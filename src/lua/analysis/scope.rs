use tree_sitter::Node;

use super::LuaDiagnostic;
use super::globals::is_handler_global;
use super::names::Names;

pub(super) fn analyze_handler(source: &str, function: Node<'_>) -> Vec<LuaDiagnostic> {
    ScopeAnalyzer::new(source, "Lua handler environment").run(Step::Function(function))
}

pub(super) fn analyze_chunk(source: &str, chunk: Node<'_>) -> Vec<LuaDiagnostic> {
    ScopeAnalyzer::new(source, "Lua code source environment").run(Step::Visit(chunk))
}

/// One unit of deferred work.
///
/// The analyzer keeps its own stack of steps instead of recursing, so the
/// depth of a syntax tree never becomes native stack depth (IF-175).
enum Step<'tree> {
    /// Analyze a node according to its kind.
    Visit(Node<'tree>),
    /// Analyze a function's parameters and body in a new scope.
    Function(Node<'tree>),
    /// Record an assignment target after its right-hand side was read.
    Assign(Node<'tree>),
    /// Declare the names of a `variable_list` in the current scope.
    DeclareList(Node<'tree>),
    /// Declare one identifier in the current scope.
    Declare(Node<'tree>),
    /// Leave the scope opened by the construct that scheduled this step.
    CloseScope,
}

struct ScopeAnalyzer<'source, 'tree> {
    source: &'source str,
    environment: &'static str,
    names: Names<'source>,
    warnings: Vec<LuaDiagnostic>,
    /// Remaining work; the next step is the last element.
    pending: Vec<Step<'tree>>,
}

impl<'source, 'tree> ScopeAnalyzer<'source, 'tree> {
    fn new(source: &'source str, environment: &'static str) -> Self {
        Self {
            source,
            environment,
            names: Names::new(source),
            warnings: Vec::new(),
            pending: Vec::new(),
        }
    }

    fn run(mut self, first: Step<'tree>) -> Vec<LuaDiagnostic> {
        self.pending.push(first);
        while let Some(step) = self.pending.pop() {
            let scheduled = self.pending.len();
            match step {
                Step::Visit(node) => self.visit(node),
                Step::Function(function) => self.function(function),
                Step::Assign(target) => self.assign(target),
                Step::DeclareList(list) => self.names.declare_variable_list(list),
                Step::Declare(name) => self.names.declare(name),
                Step::CloseScope => self.names.close_scope(),
            }
            // A step schedules its follow-up work in source order; reversing
            // that tail makes the stack hand it back in the same order.
            self.pending[scheduled..].reverse();
        }
        self.warnings
    }

    fn schedule(&mut self, step: Step<'tree>) {
        self.pending.push(step);
    }

    fn schedule_named_children(&mut self, node: Node<'tree>) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.schedule(Step::Visit(child));
        }
    }

    fn function(&mut self, function: Node<'tree>) {
        self.names.open_scope();
        if let Some(parameters) = function.child_by_field_name("parameters") {
            self.names.declare_identifiers(parameters);
        }
        if let Some(body) = function.child_by_field_name("body") {
            self.schedule_named_children(body);
        }
        self.schedule(Step::CloseScope);
    }

    fn visit(&mut self, node: Node<'tree>) {
        match node.kind() {
            "block" => {
                self.names.open_scope();
                self.schedule_named_children(node);
                self.schedule(Step::CloseScope);
            }
            "variable_declaration" => self.local_declaration(node),
            "assignment_statement" => self.assignment(node),
            "function_definition" => self.function(node),
            "function_declaration" => self.function_declaration(node),
            "for_statement" => self.for_statement(node),
            "repeat_statement" => self.repeat(node),
            "field" => self.table_field(node),
            "identifier" => self.read_identifier(node),
            "parameters" | "variable_list" | "label_statement" | "goto_statement" | "attribute" => {
            }
            _ => self.children(node),
        }
    }

    fn local_declaration(&mut self, declaration: Node<'tree>) {
        let mut cursor = declaration.walk();
        let children = declaration.named_children(&mut cursor).collect::<Vec<_>>();
        // Initializers are read before the names they define become visible.
        for child in &children {
            if child.kind() == "assignment_statement"
                && let Some(values) = child.child_by_field_name("value")
            {
                self.schedule(Step::Visit(values));
            }
        }
        for child in children {
            if child.kind() == "variable_list" {
                self.schedule(Step::DeclareList(child));
                continue;
            }
            let mut nested = child.walk();
            for target in child.named_children(&mut nested) {
                if target.kind() == "variable_list" {
                    self.schedule(Step::DeclareList(target));
                }
            }
        }
    }

    fn assignment(&mut self, assignment: Node<'tree>) {
        if let Some(values) = assignment.child_by_field_name("value") {
            self.schedule(Step::Visit(values));
        } else {
            let mut cursor = assignment.walk();
            for child in assignment.named_children(&mut cursor) {
                if child.kind() == "expression_list" {
                    self.schedule(Step::Visit(child));
                }
            }
        }

        let mut cursor = assignment.walk();
        for child in assignment.named_children(&mut cursor) {
            if child.kind() == "variable_list" {
                let mut variables = child.walk();
                for target in child.named_children(&mut variables) {
                    self.schedule(Step::Assign(target));
                }
            }
        }
    }

    fn assign(&mut self, target: Node<'tree>) {
        if target.kind() == "identifier" {
            self.names.assign(target);
        } else {
            self.schedule(Step::Visit(target));
        }
    }

    fn function_declaration(&mut self, declaration: Node<'tree>) {
        let is_local = self
            .names
            .text(declaration)
            .trim_start()
            .starts_with("local ");
        if let Some(name) = declaration.child_by_field_name("name") {
            if is_local && name.kind() == "identifier" {
                self.names.declare(name);
            } else {
                self.schedule(Step::Assign(name));
            }
        }
        self.schedule(Step::Function(declaration));
    }

    fn for_statement(&mut self, statement: Node<'tree>) {
        let Some(clause) = statement.child_by_field_name("clause") else {
            self.children(statement);
            return;
        };
        self.names.open_scope();
        match clause.kind() {
            "for_numeric_clause" => {
                for field in ["start", "end", "step"] {
                    if let Some(value) = clause.child_by_field_name(field) {
                        self.schedule(Step::Visit(value));
                    }
                }
                if let Some(name) = clause.child_by_field_name("name") {
                    self.schedule(Step::Declare(name));
                }
            }
            "for_generic_clause" => {
                let mut cursor = clause.walk();
                for child in clause.named_children(&mut cursor) {
                    if child.kind() == "expression_list" {
                        self.schedule(Step::Visit(child));
                    } else if child.kind() == "variable_list" {
                        self.schedule(Step::DeclareList(child));
                    }
                }
            }
            _ => self.schedule(Step::Visit(clause)),
        }
        if let Some(body) = statement.child_by_field_name("body") {
            self.schedule_named_children(body);
        }
        self.schedule(Step::CloseScope);
    }

    fn repeat(&mut self, statement: Node<'tree>) {
        self.names.open_scope();
        if let Some(body) = statement.child_by_field_name("body") {
            self.schedule_named_children(body);
        }
        // The condition still sees the body's locals.
        if let Some(condition) = statement.child_by_field_name("condition") {
            self.schedule(Step::Visit(condition));
        }
        self.schedule(Step::CloseScope);
    }

    fn table_field(&mut self, field: Node<'tree>) {
        if let Some(name) = field.child_by_field_name("name")
            && name.kind() != "identifier"
        {
            self.schedule(Step::Visit(name));
        }
        if let Some(value) = field.child_by_field_name("value") {
            self.schedule(Step::Visit(value));
        }
    }

    fn children(&mut self, node: Node<'tree>) {
        // In `a.b` and `a:b()` only the table expression is a variable read.
        let member_access = matches!(
            node.kind(),
            "dot_index_expression" | "method_index_expression"
        );
        let table = node.child_by_field_name("table").map(|item| item.id());
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if member_access && child.kind() == "identifier" && Some(child.id()) != table {
                continue;
            }
            self.schedule(Step::Visit(child));
        }
    }

    fn read_identifier(&mut self, node: Node<'_>) {
        let name = self.names.text(node);
        if self.names.is_known(name) || is_handler_global(name) {
            return;
        }
        self.warnings.push(LuaDiagnostic {
            code: "undefined_global".to_string(),
            message: format!("`{name}` is not defined in the {}", self.environment),
            line: node.start_position().row + 1,
            column: character_column(self.source, node.start_byte()),
            step: None,
        });
    }
}

fn character_column(source: &str, offset: usize) -> usize {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    source[line_start..offset].chars().count() + 1
}
