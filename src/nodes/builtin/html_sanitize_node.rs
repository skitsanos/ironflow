use std::collections::HashSet;

use ammonia::Builder;
use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

/// Sanitize HTML by removing dangerous tags, attributes, and scripts.
pub struct HtmlSanitizeNode;

#[async_trait]
impl Node for HtmlSanitizeNode {
    fn node_type(&self) -> &str {
        "html_sanitize"
    }

    fn description(&self) -> &str {
        "Sanitize HTML by removing dangerous tags, attributes, and scripts"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("sanitized_html");

        let strip_comments = config
            .get("strip_comments")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let link_rel = config
            .get("link_rel")
            .and_then(|v| v.as_str())
            .unwrap_or("noopener noreferrer");

        let input = get_input(config, ctx, "html_sanitize")?;

        let has_custom_tags = config
            .get("allowed_tags")
            .and_then(|v| v.as_array())
            .is_some();

        let sanitized = if has_custom_tags || !strip_comments || link_rel != "noopener noreferrer" {
            let mut builder = Builder::default();
            builder.strip_comments(strip_comments);
            builder.link_rel(Some(link_rel));

            if let Some(tags) = config.get("allowed_tags").and_then(|v| v.as_array()) {
                let tag_set: HashSet<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
                builder.tags(tag_set);
            }

            builder.clean(&input).to_string()
        } else {
            ammonia::clean(&input)
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(sanitized));
        Ok(output)
    }
}

/// Get input text from either `input` (literal string with interpolation)
/// or `source_key` (context key reference).
fn get_input(config: &serde_json::Value, ctx: &Context, node_name: &str) -> Result<String> {
    let has_input = config.get("input").and_then(|v| v.as_str()).is_some();
    let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

    if has_input && has_source_key {
        anyhow::bail!(
            "{} accepts either 'input' or 'source_key', not both",
            node_name
        );
    }

    if let Some(input_str) = config.get("input").and_then(|v| v.as_str()) {
        Ok(interpolate_ctx(input_str, ctx))
    } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
        let val = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        match val {
            serde_json::Value::String(s) => Ok(s.clone()),
            other => Ok(serde_json::to_string(other)?),
        }
    } else {
        anyhow::bail!(
            "{} requires either 'input' string or 'source_key'",
            node_name
        )
    }
}
