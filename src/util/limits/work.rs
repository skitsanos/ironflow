//! Budgets for parsing, planning and fan-out work (IF-215). Each bounds work
//! that grows faster than its input, or a reply that would otherwise enter
//! workflow context whole.

use super::Limit;

/// Values the `json_parse` node, or `json_extract_path` with `parse_json`,
/// builds from one text. Native values may exceed the Lua conversion budget
/// (IF-136), so this is a separate and larger ceiling: about 32 bytes of
/// tree per value besides the strings.
pub(crate) const MAX_JSON_PARSE_NODES: Limit =
    Limit::positive("IRONFLOW_MAX_JSON_PARSE_NODES", 1_000_000);
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
/// Bytes of one S3 list response (16 MiB). A page of 1,000 keys of the
/// longest key S3 allows is well under half of this.
pub(crate) const MAX_S3_LIST_PAGE_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_S3_LIST_PAGE_BYTES", 16 * super::MIB);

fn count(limit: &Limit) -> usize {
    usize::try_from(limit.read()).unwrap_or(usize::MAX)
}

pub fn max_json_parse_nodes() -> usize {
    count(&MAX_JSON_PARSE_NODES)
}

pub fn max_html_text_depth() -> usize {
    count(&MAX_HTML_TEXT_DEPTH)
}

pub fn max_mcp_result_bytes() -> u64 {
    MAX_MCP_RESULT_BYTES.read()
}

pub fn max_s3_list_page_bytes() -> u64 {
    MAX_S3_LIST_PAGE_BYTES.read()
}
