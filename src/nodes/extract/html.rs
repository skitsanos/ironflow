use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::execution::{ExecutionControl, run_tracked_blocking_step};

use super::common::{ensure_distinct_keys, optional_string, string_or, validate_format};
use super::resource::{Budget, Limits, read_string};
use crate::util::node_config::get_path;

pub struct ExtractHtmlNode;

#[async_trait]
impl Node for ExtractHtmlNode {
    fn node_type(&self) -> &str {
        "extract_html"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from an HTML file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = get_path(config, ctx, "extract_html")?;
        let format = validate_format(config, "extract_html")?.to_string();
        let output_key = string_or(config, "output_key", "content", "extract_html")?.to_string();
        let metadata_key =
            optional_string(config, "metadata_key", "extract_html")?.map(str::to_string);
        if let Some(metadata_key) = metadata_key.as_deref() {
            ensure_distinct_keys(
                "extract_html",
                &[("output_key", &output_key), ("metadata_key", metadata_key)],
            )?;
        }

        run_tracked_blocking_step(move |execution| {
            extract_html(&path, &format, output_key, metadata_key, &execution)
        })
        .await
    }
}

fn extract_html(
    path: &str,
    format: &str,
    output_key: String,
    metadata_key: Option<String>,
    execution: &ExecutionControl,
) -> Result<NodeOutput> {
    let limits = Limits::current();
    let mut budget = Budget::new("extract_html", limits, execution);
    let html = read_string(
        std::path::Path::new(path),
        crate::util::limits::max_file_bytes(),
        "extract_html",
        execution,
    )?;
    budget.inspect_html(&html)?;

    budget.checkpoint()?;
    let content = match format {
        "markdown" => html2md::parse_html(&html),
        _ => {
            // Strip HTML tags for plain text — sanitize with ammonia then strip.
            let clean = ammonia::clean(&html);
            budget.checkpoint()?;
            html2md::parse_html(&clean)
                .lines()
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("\n")
        }
    };
    budget.checkpoint()?;
    budget.charge_output(content.len() as u64, "HTML content")?;

    let mut output = NodeOutput::new();
    output.insert(output_key, serde_json::Value::String(content));

    if let Some(meta_key) = metadata_key {
        let metadata = extract_html_metadata(&html, &budget)?;
        output.insert(meta_key, serde_json::to_value(metadata)?);
    }
    budget.ensure_output(&output)?;
    Ok(output)
}

fn extract_html_metadata(html: &str, budget: &Budget<'_>) -> Result<BTreeMap<String, String>> {
    let mut meta = BTreeMap::new();
    budget.checkpoint()?;

    // Extract <title> content
    if let Some(start) = find_ascii_case_insensitive(html, 0, b"<title", budget)? {
        let after_tag = &html[start..];
        if let Some(close) = after_tag.find('>') {
            let after_open = &after_tag[close + 1..];
            if let Some(end) = find_ascii_case_insensitive(after_open, 0, b"</title>", budget)? {
                let title = after_open[..end].trim().to_string();
                if !title.is_empty() {
                    meta.insert("title".to_string(), title);
                }
            }
        }
    }

    // Extract <meta> tags
    let mut search_from = 0;
    while let Some(abs_pos) = find_ascii_case_insensitive(html, search_from, b"<meta ", budget)? {
        budget.checkpoint()?;
        let tag_end = match html[abs_pos..].find('>') {
            Some(p) => abs_pos + p + 1,
            None => break,
        };
        let tag = &html[abs_pos..tag_end];

        if let (Some(name), Some(content)) = (
            extract_attr(tag, "name", budget)?.or(extract_attr(tag, "property", budget)?),
            extract_attr(tag, "content", budget)?,
        ) {
            let key = name.to_lowercase();
            match key.as_str() {
                "description" | "author" | "keywords" | "viewport" | "og:title"
                | "og:description" | "og:type" | "og:url" => {
                    meta.insert(key, content);
                }
                _ => {}
            }
        }

        search_from = tag_end;
    }

    Ok(meta)
}

fn extract_attr(tag: &str, attr_name: &str, budget: &Budget<'_>) -> Result<Option<String>> {
    let pattern = format!("{}=\"", attr_name);
    if let Some(start) = find_ascii_case_insensitive(tag, 0, pattern.as_bytes(), budget)? {
        let value_start = start + pattern.len();
        if let Some(end) = tag[value_start..].find('"') {
            return Ok(Some(tag[value_start..value_start + end].to_string()));
        }
    }
    // Try single quotes
    let pattern = format!("{}='", attr_name);
    if let Some(start) = find_ascii_case_insensitive(tag, 0, pattern.as_bytes(), budget)? {
        let value_start = start + pattern.len();
        if let Some(end) = tag[value_start..].find('\'') {
            return Ok(Some(tag[value_start..value_start + end].to_string()));
        }
    }
    Ok(None)
}

fn find_ascii_case_insensitive(
    haystack: &str,
    from: usize,
    needle: &[u8],
    budget: &Budget<'_>,
) -> Result<Option<usize>> {
    let bytes = haystack.as_bytes();
    if needle.is_empty()
        || needle.len() > bytes.len()
        || from > bytes.len().saturating_sub(needle.len())
    {
        return Ok(None);
    }
    for index in from..=bytes.len() - needle.len() {
        if index % (16 * 1024) == 0 {
            budget.checkpoint()?;
        }
        if bytes[index..index + needle.len()].eq_ignore_ascii_case(needle) {
            return Ok(Some(index));
        }
    }
    Ok(None)
}
