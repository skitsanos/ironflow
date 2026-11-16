//! Markdown conversion with an output budget (IF-215).
//!
//! The converter repeats bytes that are not in the document: a block quote
//! or list item prefixes every line of its content, once per nesting level,
//! and a table with a header row pads every cell to the widest cell of its
//! column. 323 KB of nested quotes produced 20 MB and a 1 MiB header cell
//! over 200 rows produced 212 MB. The elements that repeat are therefore
//! wrapped: each wrapper charges what its element adds, and once the charge
//! or one element's own result passes the limit the remaining elements
//! convert to nothing and the call fails.

use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Result, bail};
use htmd::element_handler::{HandlerResult, Handlers};
use htmd::{Element, HtmlToMarkdown};
use markup5ever_rcdom::Node;

use super::markdown_meter::Meter;
use super::tree::parent;

/// Bytes a block quote puts before each line: `> `.
const QUOTE_PREFIX: u64 = 2;
/// Bytes an unordered item indents its continuation lines by: the marker
/// and the converter's three spaces.
const BULLET_INDENT: u64 = 4;
/// Most bytes an ordered list indents a line by: a ten-digit index, the
/// period and the converter's two spaces.
const ORDERED_INDENT: u64 = 13;

/// Convert `root` to Markdown, skipping `skip_tags`, with output charged
/// against `limit` bytes while it is produced.
pub(super) fn convert(
    root: &Rc<Node>,
    skip_tags: &[&str],
    operation: &str,
    limit: u64,
) -> Result<String> {
    let meter = Arc::new(Meter::new(limit));
    let mut builder = HtmlToMarkdown::builder();
    if !skip_tags.is_empty() {
        builder = builder.skip_tags(skip_tags.to_vec());
    }
    let converter = builder
        .add_handler(vec!["blockquote"], prefixed(&meter, |_| QUOTE_PREFIX))
        .add_handler(vec!["li"], prefixed(&meter, item_indent))
        .add_handler(vec!["ol"], prefixed(&meter, |_| ORDERED_INDENT))
        .add_handler(vec!["table"], table(&meter))
        .add_handler(vec!["td", "th"], cell(&meter))
        .build();
    let markdown = converter.tree_to_markdown(root);
    if meter.exceeded() || markdown.len() as u64 > limit {
        bail!("{operation}: Markdown output exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES ({limit})");
    }
    Ok(markdown)
}

type Handler = Box<dyn Fn(&dyn Handlers, Element) -> Option<HandlerResult> + Send + Sync>;

/// An ordered list indents its own items, so an item only indents under
/// any other parent.
fn item_indent(node: &Rc<Node>) -> u64 {
    match parent(node).as_deref().and_then(tag_of) {
        Some("ol") => 0,
        _ => BULLET_INDENT,
    }
}

fn tag_of(node: &Node) -> Option<&str> {
    match &node.data {
        markup5ever_rcdom::NodeData::Element { name, .. } => Some(name.local.as_ref()),
        _ => None,
    }
}

/// Wrap an element that prefixes each line of its content with at most
/// `per_line(node)` bytes.
fn prefixed(meter: &Arc<Meter>, per_line: fn(&Rc<Node>) -> u64) -> Handler {
    let meter = Arc::clone(meter);
    Box::new(move |handlers, element| {
        if meter.exceeded() {
            return None;
        }
        let per_line = per_line(element.node);
        let result = handlers.fallback(element)?;
        // `matches` searches with the standard library's optimized scan.
        let lines = result.content.matches('\n').count() as u64 + 1;
        meter
            .charge(per_line.saturating_mul(lines), result.content.len())
            .then_some(result)
    })
}

fn table(meter: &Arc<Meter>) -> Handler {
    let meter = Arc::clone(meter);
    Box::new(move |handlers, element| {
        if meter.exceeded() {
            return None;
        }
        meter.enter_table(element.node);
        let result = handlers.fallback(element);
        let length = result.as_ref().map_or(0, |result| result.content.len());
        meter.leave_table(length).then_some(result).flatten()
    })
}

fn cell(meter: &Arc<Meter>) -> Handler {
    let meter = Arc::clone(meter);
    Box::new(move |handlers, element| {
        if meter.exceeded() {
            return None;
        }
        let row = parent(element.node).map_or(0, |row| Rc::as_ptr(&row) as usize);
        let result = handlers.fallback(element)?;
        meter.cell(row, &result.content).then_some(result)
    })
}
