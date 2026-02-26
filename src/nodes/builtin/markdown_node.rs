use anyhow::Result;
use async_trait::async_trait;
use comrak::{markdown_to_html, Options};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

/// Convert Markdown to HTML using comrak (CommonMark + GFM).
pub struct MarkdownToHtmlNode;

#[async_trait]
impl Node for MarkdownToHtmlNode {
    fn node_type(&self) -> &str {
        "markdown_to_html"
    }

    fn description(&self) -> &str {
        "Convert Markdown text to HTML"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("html");

        let sanitize = config
            .get("sanitize")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let input = get_input(config, &ctx, "markdown_to_html")?;

        let mut options = Options::default();
        options.extension.strikethrough = true;
        options.extension.table = true;
        options.extension.autolink = true;
        options.extension.tasklist = true;

        let html = markdown_to_html(&input, &options);

        let html = if sanitize {
            ammonia::clean(&html)
        } else {
            html
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(html));
        Ok(output)
    }
}

/// Convert HTML to Markdown (best-effort, inherently lossy on complex HTML).
pub struct HtmlToMarkdownNode;

#[async_trait]
impl Node for HtmlToMarkdownNode {
    fn node_type(&self) -> &str {
        "html_to_markdown"
    }

    fn description(&self) -> &str {
        "Convert HTML to Markdown (best-effort, lossy on complex HTML)"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("markdown");

        let input = get_input(config, &ctx, "html_to_markdown")?;

        let markdown = html2md::parse_html(&input);

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::Value::String(markdown),
        );
        Ok(output)
    }
}

/// Get input text from either `input` (literal string with interpolation)
/// or `source_key` (context key reference).
fn get_input(config: &serde_json::Value, ctx: &Context, node_name: &str) -> Result<String> {
    let has_input = config.get("input").and_then(|v| v.as_str()).is_some();
    let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

    if has_input && has_source_key {
        anyhow::bail!("{} accepts either 'input' or 'source_key', not both", node_name);
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
        anyhow::bail!("{} requires either 'input' string or 'source_key'", node_name)
    }
}
