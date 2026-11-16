//! Names visible to the Lua code being analyzed.

use std::collections::HashSet;

use tree_sitter::Node;

/// Lexical scopes plus the globals the analyzed source assigns itself.
pub(super) struct Names<'source> {
    source: &'source str,
    scopes: Vec<HashSet<String>>,
    assigned_globals: HashSet<String>,
}

impl<'source> Names<'source> {
    pub(super) fn new(source: &'source str) -> Self {
        Self {
            source,
            scopes: vec![HashSet::new()],
            assigned_globals: HashSet::new(),
        }
    }

    pub(super) fn text(&self, node: Node<'_>) -> &'source str {
        &self.source[node.byte_range()]
    }

    pub(super) fn open_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    pub(super) fn close_scope(&mut self) {
        self.scopes.pop();
    }

    /// Whether `name` is a visible local or a global assigned earlier.
    pub(super) fn is_known(&self, name: &str) -> bool {
        self.is_local(name) || self.assigned_globals.contains(name)
    }

    /// Record an assignment to a bare identifier: a new global unless a local
    /// of that name is visible.
    pub(super) fn assign(&mut self, target: Node<'_>) {
        let name = self.text(target);
        if !self.is_local(name) {
            self.assigned_globals.insert(name.to_string());
        }
    }

    #[allow(clippy::expect_used)] // Infallible: scope analyzer always has a scope.
    pub(super) fn declare(&mut self, identifier: Node<'_>) {
        let name = self.text(identifier).to_string();
        self.scopes
            .last_mut()
            .expect("scope analyzer always has a scope")
            .insert(name);
    }

    /// Declare every identifier below `node`, such as a parameter list.
    pub(super) fn declare_identifiers(&mut self, node: Node<'_>) {
        let mut pending = vec![node];
        while let Some(node) = pending.pop() {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "identifier" {
                    self.declare(child);
                } else {
                    pending.push(child);
                }
            }
        }
    }

    /// Declare the names of a `variable_list`, including attributed names.
    pub(super) fn declare_variable_list(&mut self, list: Node<'_>) {
        if list.kind() != "variable_list" {
            return;
        }
        let mut cursor = list.walk();
        for variable in list.named_children(&mut cursor) {
            if variable.kind() == "identifier" {
                self.declare(variable);
                continue;
            }
            let mut nested = variable.walk();
            for child in variable.named_children(&mut nested) {
                if child.kind() == "identifier" {
                    self.declare(child);
                }
            }
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }
}
