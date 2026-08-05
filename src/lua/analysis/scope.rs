use std::collections::HashSet;

use tree_sitter::Node;

use super::LuaDiagnostic;
use super::globals::is_handler_global;

pub(super) fn analyze_handler(source: &str, function: Node<'_>) -> Vec<LuaDiagnostic> {
    let mut analyzer = new_analyzer(source, "Lua handler environment");
    analyzer.analyze_function(function);
    analyzer.warnings
}

pub(super) fn analyze_chunk(source: &str, chunk: Node<'_>) -> Vec<LuaDiagnostic> {
    let mut analyzer = new_analyzer(source, "Lua code source environment");
    analyzer.analyze_node(chunk);
    analyzer.warnings
}

fn new_analyzer<'a>(source: &'a str, environment: &'static str) -> ScopeAnalyzer<'a> {
    ScopeAnalyzer {
        source,
        environment,
        scopes: vec![HashSet::new()],
        assigned_globals: HashSet::new(),
        warnings: Vec::new(),
    }
}

struct ScopeAnalyzer<'source> {
    source: &'source str,
    environment: &'static str,
    scopes: Vec<HashSet<String>>,
    assigned_globals: HashSet<String>,
    warnings: Vec<LuaDiagnostic>,
}

impl ScopeAnalyzer<'_> {
    fn analyze_function(&mut self, function: Node<'_>) {
        self.scopes.push(HashSet::new());
        if let Some(parameters) = function.child_by_field_name("parameters") {
            self.declare_identifiers(parameters);
        }
        if let Some(body) = function.child_by_field_name("body") {
            self.analyze_block(body, false);
        }
        self.scopes.pop();
    }

    fn analyze_block(&mut self, block: Node<'_>, nested_scope: bool) {
        if nested_scope {
            self.scopes.push(HashSet::new());
        }
        let mut cursor = block.walk();
        for statement in block.named_children(&mut cursor) {
            self.analyze_node(statement);
        }
        if nested_scope {
            self.scopes.pop();
        }
    }

    fn analyze_node(&mut self, node: Node<'_>) {
        match node.kind() {
            "block" => self.analyze_block(node, true),
            "variable_declaration" => self.analyze_local_declaration(node),
            "assignment_statement" => self.analyze_assignment(node),
            "function_definition" => self.analyze_function(node),
            "function_declaration" => self.analyze_function_declaration(node),
            "for_statement" => self.analyze_for(node),
            "repeat_statement" => self.analyze_repeat(node),
            "field" => self.analyze_table_field(node),
            "identifier" => self.read_identifier(node),
            "parameters" | "variable_list" | "label_statement" | "goto_statement" | "attribute" => {
            }
            _ => self.analyze_children(node),
        }
    }

    fn analyze_local_declaration(&mut self, declaration: Node<'_>) {
        let mut cursor = declaration.walk();
        let children = declaration.named_children(&mut cursor).collect::<Vec<_>>();
        for child in &children {
            if child.kind() == "assignment_statement"
                && let Some(values) = child.child_by_field_name("value")
            {
                self.analyze_node(values);
            }
        }
        for child in children {
            if child.kind() == "variable_list" {
                self.declare_names_in_variable_list(child);
                continue;
            }
            let mut nested = child.walk();
            for target in child.named_children(&mut nested) {
                if target.kind() == "variable_list" {
                    self.declare_names_in_variable_list(target);
                }
            }
        }
    }

    fn analyze_assignment(&mut self, assignment: Node<'_>) {
        if let Some(values) = assignment.child_by_field_name("value") {
            self.analyze_node(values);
        } else {
            let mut cursor = assignment.walk();
            for child in assignment.named_children(&mut cursor) {
                if child.kind() == "expression_list" {
                    self.analyze_node(child);
                }
            }
        }

        let mut cursor = assignment.walk();
        for child in assignment.named_children(&mut cursor) {
            if child.kind() == "variable_list" {
                let mut variables = child.walk();
                for target in child.named_children(&mut variables) {
                    self.analyze_assignment_target(target);
                }
            }
        }
    }

    fn analyze_assignment_target(&mut self, target: Node<'_>) {
        if target.kind() == "identifier" {
            let name = self.text(target);
            if !self.is_defined(name) {
                self.assigned_globals.insert(name.to_string());
            }
        } else {
            self.analyze_node(target);
        }
    }

    fn analyze_function_declaration(&mut self, declaration: Node<'_>) {
        let is_local = self.text(declaration).trim_start().starts_with("local ");
        if let Some(name) = declaration.child_by_field_name("name") {
            if is_local && name.kind() == "identifier" {
                self.declare_node(name);
            } else {
                self.analyze_assignment_target(name);
            }
        }
        self.analyze_function(declaration);
    }

    fn analyze_for(&mut self, statement: Node<'_>) {
        let Some(clause) = statement.child_by_field_name("clause") else {
            self.analyze_children(statement);
            return;
        };
        self.scopes.push(HashSet::new());
        match clause.kind() {
            "for_numeric_clause" => {
                for field in ["start", "end", "step"] {
                    if let Some(value) = clause.child_by_field_name(field) {
                        self.analyze_node(value);
                    }
                }
                if let Some(name) = clause.child_by_field_name("name") {
                    self.declare_node(name);
                }
            }
            "for_generic_clause" => {
                let mut cursor = clause.walk();
                for child in clause.named_children(&mut cursor) {
                    if child.kind() == "expression_list" {
                        self.analyze_node(child);
                    } else if child.kind() == "variable_list" {
                        self.declare_names_in_variable_list(child);
                    }
                }
            }
            _ => self.analyze_node(clause),
        }
        if let Some(body) = statement.child_by_field_name("body") {
            self.analyze_block(body, false);
        }
        self.scopes.pop();
    }

    fn analyze_repeat(&mut self, statement: Node<'_>) {
        self.scopes.push(HashSet::new());
        if let Some(body) = statement.child_by_field_name("body") {
            self.analyze_block(body, false);
        }
        if let Some(condition) = statement.child_by_field_name("condition") {
            self.analyze_node(condition);
        }
        self.scopes.pop();
    }

    fn analyze_table_field(&mut self, field: Node<'_>) {
        if let Some(name) = field.child_by_field_name("name")
            && name.kind() != "identifier"
        {
            self.analyze_node(name);
        }
        if let Some(value) = field.child_by_field_name("value") {
            self.analyze_node(value);
        }
    }

    fn analyze_children(&mut self, node: Node<'_>) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if matches!(
                node.kind(),
                "dot_index_expression" | "method_index_expression"
            ) && matches!(child.kind(), "identifier")
                && child.id()
                    != node
                        .child_by_field_name("table")
                        .map(|item| item.id())
                        .unwrap_or(0)
            {
                continue;
            }
            self.analyze_node(child);
        }
    }

    fn declare_identifiers(&mut self, node: Node<'_>) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "identifier" {
                self.declare_node(child);
            } else {
                self.declare_identifiers(child);
            }
        }
    }

    fn declare_names_in_variable_list(&mut self, node: Node<'_>) {
        if node.kind() != "variable_list" {
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.declare_variable_tree(child);
        }
    }

    fn declare_variable_tree(&mut self, node: Node<'_>) {
        if node.kind() == "identifier" {
            self.declare_node(node);
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "identifier" {
                self.declare_node(child);
            }
        }
    }

    fn read_identifier(&mut self, node: Node<'_>) {
        let name = self.text(node);
        if self.is_defined(name) || self.assigned_globals.contains(name) || is_handler_global(name)
        {
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

    fn declare(&mut self, name: &str) {
        self.scopes
            .last_mut()
            .expect("scope analyzer always has a scope")
            .insert(name.to_string());
    }

    fn declare_node(&mut self, node: Node<'_>) {
        let name = self.text(node).to_string();
        self.declare(&name);
    }

    fn is_defined(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn text(&self, node: Node<'_>) -> &str {
        &self.source[node.byte_range()]
    }
}

fn character_column(source: &str, offset: usize) -> usize {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    source[line_start..offset].chars().count() + 1
}
