use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::common::{get_path, validate_format};

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
        let format = validate_format(config, "extract_html")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("content");
        let metadata_key = config.get("metadata_key").and_then(|v| v.as_str());

        let html = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;

        let content = match format {
            "markdown" => html2md::parse_html(&html),
            _ => {
                // Strip HTML tags for plain text — sanitize with ammonia then strip
                let clean = ammonia::clean(&html);
                // ammonia keeps safe HTML; parse again with html2md for text extraction
                html2md::parse_html(&clean)
                    .lines()
                    .map(|l| l.trim())
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(content));

        if let Some(meta_key) = metadata_key {
            let metadata = extract_html_metadata(&html);
            output.insert(meta_key.to_string(), serde_json::to_value(metadata)?);
        }
        Ok(output)
    }
}

fn extract_html_metadata(html: &str) -> BTreeMap<String, String> {
    let mut meta = BTreeMap::new();

    // Extract <title> content
    if let Some(start) = html.find("<title>").or_else(|| html.find("<title ")) {
        let after_tag = &html[start..];
        if let Some(close) = after_tag.find('>') {
            let after_open = &after_tag[close + 1..];
            if let Some(end) = after_open.find("</title>") {
                let title = after_open[..end].trim().to_string();
                if !title.is_empty() {
                    meta.insert("title".to_string(), title);
                }
            }
        }
    }

    // Extract <meta> tags
    let lower = html.to_lowercase();
    let mut search_from = 0;
    while let Some(pos) = lower[search_from..].find("<meta ") {
        let abs_pos = search_from + pos;
        let tag_end = match lower[abs_pos..].find('>') {
            Some(p) => abs_pos + p + 1,
            None => break,
        };
        let tag = &html[abs_pos..tag_end];

        if let (Some(name), Some(content)) = (
            extract_attr(tag, "name").or_else(|| extract_attr(tag, "property")),
            extract_attr(tag, "content"),
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

    meta
}

fn extract_attr(tag: &str, attr_name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let pattern = format!("{}=\"", attr_name);
    if let Some(start) = lower.find(&pattern) {
        let value_start = start + pattern.len();
        if let Some(end) = tag[value_start..].find('"') {
            return Some(tag[value_start..value_start + end].to_string());
        }
    }
    // Try single quotes
    let pattern = format!("{}='", attr_name);
    if let Some(start) = lower.find(&pattern) {
        let value_start = start + pattern.len();
        if let Some(end) = tag[value_start..].find('\'') {
            return Some(tag[value_start..value_start + end].to_string());
        }
    }
    None
}
