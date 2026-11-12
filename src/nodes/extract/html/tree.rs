//! Bounded HTML parsing shared by `extract_html` and `html_to_markdown`.
//!
//! The Markdown converter recurses once per element level, and the HTML tree
//! builder rescans its open elements for every tag, so parse time grows with
//! the square of the nesting depth. Nesting is therefore capped while the
//! document is still being parsed (IF-174).

use std::cell::Cell;
use std::rc::Rc;

use anyhow::{Result, bail};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::{Tracer, TreeBuilderOpts};
use html5ever::{ParseOpts, parse_document};
use markup5ever_rcdom::{Handle, Node, NodeData, RcDom};

use crate::util::execution::ExecutionControl;

/// Deepest element nesting accepted before Markdown conversion, counting the
/// `<html>` and `<body>` elements the parser always supplies.
pub(super) const MAX_MARKDOWN_DEPTH: usize = 128;

/// Input reaches the parser in pieces so cancellation is observed and the
/// open elements are inspected between them.
const PARSE_CHUNK_BYTES: usize = 4 * 1024;

/// Parse `html` with the HTML5 recovery rules.
///
/// With `depth_cap`, a document whose elements nest deeper than the cap is a
/// node error. The finished tree decides the exact boundary; parsing only
/// stops early once an open element is twice as deep, because tree
/// construction can still move a node after opening it.
pub(super) fn parse(
    html: &str,
    operation: &str,
    depth_cap: Option<usize>,
    execution: &ExecutionControl,
) -> Result<Rc<Node>> {
    let mut parser = parse_document(
        RcDom::default(),
        ParseOpts {
            tree_builder: TreeBuilderOpts {
                scripting_enabled: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut rest = html;
    while !rest.is_empty() {
        execution.checkpoint()?;
        let (chunk, tail) = rest.split_at(chunk_end(rest));
        parser.process(StrTendril::from_slice(chunk));
        rest = tail;
        if let Some(cap) = depth_cap {
            let open = OpenElements::new(cap.saturating_mul(2));
            parser.tokenizer.sink.trace_handles(&open);
            if open.exceeded.get() {
                bail!(too_deep(operation, cap));
            }
        }
    }
    execution.checkpoint()?;
    let document = parser.finish().document;
    if let Some(cap) = depth_cap {
        ensure_depth(&document, operation, cap, execution)?;
    }
    Ok(document)
}

fn too_deep(operation: &str, cap: usize) -> String {
    format!("{operation}: HTML element nesting exceeds {cap} levels")
}

/// Largest prefix of `text` within the chunk size that ends on a character.
fn chunk_end(text: &str) -> usize {
    let mut end = text.len().min(PARSE_CHUNK_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// Receives the tree builder's open elements between input chunks.
struct OpenElements {
    limit: usize,
    exceeded: Cell<bool>,
}

impl OpenElements {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            exceeded: Cell::new(false),
        }
    }
}

impl Tracer for OpenElements {
    type Handle = Handle;

    fn trace_handle(&self, node: &Handle) {
        if !self.exceeded.get() && ancestor_elements(node, self.limit) > self.limit {
            self.exceeded.set(true);
        }
    }
}

/// Element depth of `node`, counted through its ancestors and abandoned as
/// soon as it passes `limit`.
fn ancestor_elements(node: &Handle, limit: usize) -> usize {
    let mut depth = 0;
    let mut current = Some(node.clone());
    while let Some(node) = current {
        if matches!(node.data, NodeData::Element { .. }) {
            depth += 1;
            if depth > limit {
                break;
            }
        }
        current = parent(&node);
    }
    depth
}

fn parent(node: &Node) -> Option<Handle> {
    let weak = node.parent.take()?;
    let parent = weak.upgrade();
    node.parent.set(Some(weak));
    parent
}

/// Reject a finished tree whose elements nest deeper than `cap`. Inert
/// `<template>` content counts as nested inside its template element.
fn ensure_depth(
    document: &Handle,
    operation: &str,
    cap: usize,
    execution: &ExecutionControl,
) -> Result<()> {
    let mut pending = vec![(document.clone(), 0_usize)];
    let mut visited = 0_usize;
    while let Some((node, parent_depth)) = pending.pop() {
        visited += 1;
        if visited.is_multiple_of(4096) {
            execution.checkpoint()?;
        }
        let mut depth = parent_depth;
        if let NodeData::Element {
            template_contents, ..
        } = &node.data
        {
            depth += 1;
            if depth > cap {
                bail!(too_deep(operation, cap));
            }
            if let Some(contents) = template_contents.borrow().as_ref() {
                pending.push((contents.clone(), depth));
            }
        }
        pending.extend(
            node.children
                .borrow()
                .iter()
                .map(|child| (child.clone(), depth)),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use htmd::HtmlToMarkdown;

    use super::*;

    #[test]
    fn chunks_end_on_a_character_boundary() {
        let text = format!("{}\u{1F600}tail", "a".repeat(PARSE_CHUNK_BYTES - 1));
        assert_eq!(chunk_end(&text), PARSE_CHUNK_BYTES - 1);
        assert_eq!(chunk_end("short"), 5);
    }

    #[tokio::test]
    async fn chunked_parsing_matches_the_converter_parser() {
        // The block length is coprime with the chunk size, so chunk boundaries
        // fall inside tags, attributes, entities, comments, CRLF pairs and
        // multi-byte characters over the course of the document.
        let block = concat!(
            "<p class=\"k\">caf\u{e9} &amp; &#x1F600; \u{1F600} <b>bold</b>\r\n",
            "<!-- note --><a href=\"https://example.com/?a=1&amp;b=2\">links</a></p>\r\n",
            "<pre>line one\r\nline two</pre><ul><li>item &lt;x&gt;</li></ul>\r\n",
        );
        let html = block.repeat(300);
        assert!(html.len() > 8 * PARSE_CHUNK_BYTES && block.len() % 2 == 1);

        let (chunked, one_shot) = crate::util::execution::run_blocking_step(move |execution| {
            let converter = HtmlToMarkdown::new();
            let chunked = parse(&html, "test", Some(MAX_MARKDOWN_DEPTH), &execution)?;
            let one_shot = converter.html_to_tree(&html)?;
            Ok((
                converter.tree_to_markdown(&chunked),
                converter.tree_to_markdown(&one_shot),
            ))
        })
        .await
        .unwrap();
        assert!(chunked.contains("caf\u{e9} & \u{1F600} \u{1F600} **bold**"));
        assert_eq!(chunked, one_shot);
    }
}
