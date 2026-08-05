mod globals;
mod scope;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde::Serialize;
use tree_sitter::{Node, Parser};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LuaDiagnostic {
    pub code: String,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone)]
pub(crate) struct HandlerDiagnostics {
    state: Arc<Mutex<AnalysisState>>,
}

struct AnalysisState {
    functions: Vec<FunctionAnalysis>,
    warnings: Vec<LuaDiagnostic>,
}

struct FunctionAnalysis {
    start_line: usize,
    end_line: usize,
    claimed: bool,
    warnings: Vec<LuaDiagnostic>,
}

impl HandlerDiagnostics {
    pub(crate) fn analyze(source: &str) -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_lua::LANGUAGE.into())
            .map_err(|error| {
                anyhow::anyhow!("Failed to initialize Lua source analysis: {error}")
            })?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Lua source analysis did not produce a syntax tree"))?;
        if tree.root_node().has_error() {
            return Ok(Self::empty());
        }

        let mut functions = Vec::new();
        collect_functions(source, tree.root_node(), &mut functions);
        Ok(Self {
            state: Arc::new(Mutex::new(AnalysisState {
                functions,
                warnings: Vec::new(),
            })),
        })
    }

    fn empty() -> Self {
        Self {
            state: Arc::new(Mutex::new(AnalysisState {
                functions: Vec::new(),
                warnings: Vec::new(),
            })),
        }
    }

    pub(crate) fn record_handler(&self, line: usize, end_line: usize) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Lua handler analysis lock was poisoned"))?;
        let Some(index) = state.functions.iter().position(|candidate| {
            !candidate.claimed && candidate.start_line == line && candidate.end_line == end_line
        }) else {
            anyhow::bail!(
                "Could not map serialized Lua handler at lines {line}-{end_line} to its source"
            );
        };
        state.functions[index].claimed = true;
        let diagnostics = state.functions[index].warnings.clone();
        for diagnostic in diagnostics {
            if !state.warnings.contains(&diagnostic) {
                state.warnings.push(diagnostic);
            }
        }
        Ok(())
    }

    pub(crate) fn warnings(&self) -> Result<Vec<LuaDiagnostic>> {
        let mut warnings = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Lua handler analysis lock was poisoned"))?
            .warnings
            .clone();
        warnings.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
        Ok(warnings)
    }
}

fn collect_functions(source: &str, node: Node<'_>, functions: &mut Vec<FunctionAnalysis>) {
    if matches!(node.kind(), "function_definition" | "function_declaration") {
        functions.push(FunctionAnalysis {
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            claimed: false,
            warnings: scope::analyze_handler(source, node),
        });
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_functions(source, child, functions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_only_undefined_reads_inside_claimed_handlers() {
        let source = r#"
local flow = Flow.new("lint")
local outside = missing_outside
flow:step("render", function(ctx)
    local value = string.format("%s", missing_inside)
    return { value = value, id = uuid4(), context = ctx }
end)
return flow
"#;
        let analysis = HandlerDiagnostics::analyze(source).unwrap();
        analysis.record_handler(4, 7).unwrap();
        let warnings = analysis.warnings().unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].message,
            "`missing_inside` is not defined in the Lua handler environment"
        );
        assert_eq!((warnings[0].line, warnings[0].column), (5, 39));
    }

    #[test]
    fn handler_runtime_does_not_allow_flow_loader_globals() {
        let source = r#"
local flow = Flow.new("lint")
flow:step("render", function()
    return { leaked = Flow, factory = nodes }
end)
return flow
"#;
        let analysis = HandlerDiagnostics::analyze(source).unwrap();
        analysis.record_handler(3, 5).unwrap();
        let warnings = analysis.warnings().unwrap();
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().any(|item| item.message.contains("`Flow`")));
        assert!(warnings.iter().any(|item| item.message.contains("`nodes`")));
    }

    #[test]
    fn understands_handler_lexical_scopes_and_non_variable_identifiers() {
        let source = r#"local flow = Flow.new("scopes")
flow:step("render", function(ctx)
    local left, total = ctx.left, 0
    for index, item in ipairs(ctx.items) do
        local upper = string.upper(item.name)
        total = total + index + #upper
    end
    repeat
        local complete = total > 0
        total = total + 1
    until complete
    local nested = function(value) return value + total end
    return { [ctx.key] = left, total = nested(total) }
end)
return flow
"#;
        let analysis = HandlerDiagnostics::analyze(source).unwrap();
        analysis.record_handler(2, 14).unwrap();
        assert_eq!(analysis.warnings().unwrap(), Vec::new());
    }

    #[test]
    fn local_names_are_not_visible_in_their_own_initializers() {
        let source = r#"local flow = Flow.new("initializer")
flow:step("render", function()
    local missing = missing
    return { missing = missing }
end)
return flow
"#;
        let analysis = HandlerDiagnostics::analyze(source).unwrap();
        analysis.record_handler(2, 5).unwrap();
        let warnings = analysis.warnings().unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
    }

    #[test]
    fn invalid_syntax_is_left_to_the_lua_parser() {
        let analysis = HandlerDiagnostics::analyze("return function( ???").unwrap();
        assert_eq!(analysis.warnings().unwrap(), Vec::new());
    }
}
