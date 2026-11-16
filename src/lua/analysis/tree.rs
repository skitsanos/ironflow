//! Bounded, non-recursive traversal of a Lua syntax tree.
//!
//! Operator and index chains parse into a tree as deep as the chain is long,
//! and Lua compiles them without nesting, so flow source decides the depth.
//! The analyzers therefore never recurse over the tree, and a depth cap keeps
//! their total work proportional to the source size (IF-175).

use anyhow::Result;
use tree_sitter::{Node, Tree};

/// Deepest syntax-tree nesting the analyzers accept.
///
/// Lua rejects more than about 200 nested constructs and each construct is at
/// most four tree levels, so only operator, index and call chains longer than
/// roughly a thousand links can reach this.
pub(super) const MAX_SYNTAX_DEPTH: usize = 1_000;

/// Fail when `tree` nests deeper than [`MAX_SYNTAX_DEPTH`].
pub(super) fn ensure_depth(tree: &Tree) -> Result<()> {
    let mut cursor = tree.walk();
    let mut depth = 0_usize;
    loop {
        if cursor.goto_first_child() {
            depth += 1;
            if depth > MAX_SYNTAX_DEPTH {
                anyhow::bail!(
                    "Lua source is nested too deeply for validation: line {} exceeds {MAX_SYNTAX_DEPTH} levels of syntax nesting",
                    cursor.node().start_position().row + 1
                );
            }
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return Ok(());
            }
            depth -= 1;
        }
    }
}

/// Call `visit` for every node of `tree` in document order.
pub(super) fn for_each_node<'tree>(tree: &'tree Tree, mut visit: impl FnMut(Node<'tree>)) {
    let mut cursor = tree.walk();
    loop {
        visit(cursor.node());
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}
