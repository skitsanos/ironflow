mod globals;
mod names;
mod scope;
mod tree;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde::Serialize;
use tree_sitter::Parser;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LuaDiagnostic {
    pub code: String,
    pub message: String,
    /// One-based line within the flow file or embedded code source.
    pub line: usize,
    /// One-based character column within the diagnostic line.
    pub column: usize,
    /// Step name when `line` and `column` are relative to embedded code source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
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
    recorded: bool,
    warnings: Vec<LuaDiagnostic>,
}

impl HandlerDiagnostics {
    pub(crate) fn analyze(source: &str) -> Result<Self> {
        let tree = parse_lua(source)?;
        if tree.root_node().has_error() {
            return Ok(Self::empty());
        }

        tree::ensure_depth(&tree)?;
        let mut functions = Vec::new();
        tree::for_each_node(&tree, |node| {
            if matches!(node.kind(), "function_definition" | "function_declaration") {
                functions.push(FunctionAnalysis {
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    recorded: false,
                    warnings: scope::analyze_handler(source, node),
                });
            }
        });
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
        let mut candidates = state.functions.iter().enumerate().filter(|(_, candidate)| {
            candidate.start_line == line && candidate.end_line == end_line
        });
        let Some((index, _)) = candidates.next() else {
            anyhow::bail!(
                "Could not map serialized Lua handler at lines {line}-{end_line} to its source"
            );
        };
        // Lua reports line ranges, not columns or source registration order.
        if candidates.next().is_some() {
            anyhow::bail!(
                "Ambiguous serialized Lua handler at lines {line}-{end_line}: multiple source functions share this range; define them on distinct lines"
            );
        }
        if state.functions[index].recorded {
            return Ok(());
        }
        state.functions[index].recorded = true;
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

pub(crate) fn analyze_code_source(source: &str, step: &str) -> Result<Vec<LuaDiagnostic>> {
    let tree = parse_lua(source)?;
    let (analysis_source, tree, expression_prefix) = if tree.root_node().has_error() {
        let expression_source = format!("return {source}");
        let expression_tree = parse_lua(&expression_source)?;
        if expression_tree.root_node().has_error() {
            return Ok(Vec::new());
        }
        (expression_source, expression_tree, "return ".len())
    } else {
        (source.to_string(), tree, 0)
    };
    tree::ensure_depth(&tree)
        .map_err(|error| anyhow::anyhow!("Step '{step}' code source: {error}"))?;

    let mut warnings = scope::analyze_chunk(&analysis_source, tree.root_node());
    for warning in &mut warnings {
        warning.step = Some(step.to_string());
        if warning.line == 1 {
            warning.column = warning.column.saturating_sub(expression_prefix);
        }
    }
    Ok(warnings)
}

fn parse_lua(source: &str) -> Result<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .map_err(|error| anyhow::anyhow!("Failed to initialize Lua source analysis: {error}"))?;
    parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("Lua source analysis did not produce a syntax tree"))
}

#[cfg(test)]
mod tests;
