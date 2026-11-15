//! Budgets for parsing, planning and fan-out work (IF-215). Each bounds work
//! that grows faster than its input, or a reply that would otherwise enter
//! workflow context whole.

use super::Limit;

/// Element nesting `extract_html` accepts for `format = "text"`, counting
/// the `<html>` and `<body>` the parser supplies. HTML parsing is quadratic
/// in nesting depth, so this is a time bound and not only a shape rule.
pub(crate) const MAX_HTML_TEXT_DEPTH: Limit =
    Limit::positive("IRONFLOW_MAX_HTML_TEXT_DEPTH", 1_024);
/// JSON size of the result one `mcp_client` action may put into workflow
/// context (4 MiB). The transports' own ceilings bound the reply on the
/// wire; this one bounds what the workflow keeps.
pub(crate) const MAX_MCP_RESULT_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_MCP_RESULT_BYTES", 4 * super::MIB);

fn count(limit: &Limit) -> usize {
    usize::try_from(limit.read()).unwrap_or(usize::MAX)
}

pub fn max_html_text_depth() -> usize {
    count(&MAX_HTML_TEXT_DEPTH)
}

pub fn max_mcp_result_bytes() -> u64 {
    MAX_MCP_RESULT_BYTES.read()
}
