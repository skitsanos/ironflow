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

use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use htmd::HtmlToMarkdown;
use htmd::element_handler::{Element, HandlerResult, Handlers};
use htmd::node::Node as Converted;
use markup5ever_rcdom::Node;

use super::markdown_links::Links;
use super::markdown_meter::Meter;

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
///
/// `root` is the tree IronFlow parsed under its nesting cap; the converter
/// never parses HTML itself here.
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
        .add_handler(vec!["a"], Links::default())
        .build();
    let markdown = converter
        .convert(root)
        .with_context(|| format!("{operation}: Markdown conversion failed"))?;
    if meter.exceeded() || markdown.len() as u64 > limit {
        bail!("{operation}: Markdown output exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES ({limit})");
    }
    Ok(markdown)
}

type Handler = Box<dyn Fn(&dyn Handlers, &Element) -> Option<HandlerResult> + Send + Sync>;

fn has_tag(node: Option<&Converted<'_>>, tag: &str) -> bool {
    node.and_then(Converted::tag_name) == Some(tag)
}

/// An ordered list indents its own items, so an item only indents under
/// any other parent.
fn item_indent(element: &Element) -> u64 {
    if has_tag(element.node.parent().as_ref(), "ol") {
        0
    } else {
        BULLET_INDENT
    }
}

/// Wrap an element that prefixes each line of its content with at most
/// `per_line(element)` bytes.
fn prefixed(meter: &Arc<Meter>, per_line: fn(&Element) -> u64) -> Handler {
    let meter = Arc::clone(meter);
    Box::new(move |handlers, element| {
        if meter.exceeded() {
            return None;
        }
        let per_line = per_line(element);
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
        meter.enter_table(is_padded(element.node));
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
        let row = element.node.parent().map_or(0, |row| identity(&row));
        let result = handlers.fallback(element)?;
        meter.cell(row, &result.content).then_some(result)
    })
}

/// A value that differs between two nodes of one tree. The converter's node
/// hashes by address and exposes no other identity.
fn identity(node: &Converted<'_>) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    node.hash(&mut hasher);
    hasher.finish()
}

/// The converter writes the padded pipe form for a table that has a header
/// cell or a `<thead>` of its own and is not inside another table's cell;
/// any other table is converted like ordinary blocks.
fn is_padded(table: &Converted<'_>) -> bool {
    has_header(table) && !inside_cell(table)
}

/// Recurses once per element level, like the converter, so the nesting cap
/// bounds it.
fn has_header(node: &Converted<'_>) -> bool {
    node.children().iter().any(|child| match child.tag_name() {
        Some("th" | "thead") => true,
        // A nested table's headers are its own.
        Some("table") => false,
        _ => has_header(&child),
    })
}

fn inside_cell(table: &Converted<'_>) -> bool {
    let mut current = table.parent();
    while let Some(node) = current {
        if matches!(node.tag_name(), Some("td" | "th")) {
            return true;
        }
        current = node.parent();
    }
    false
}
