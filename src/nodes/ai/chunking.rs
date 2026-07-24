mod cues;
mod fixed;
mod split;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::node_config::{config_bool, config_usize_strict};

use cues::chunk_cues;
use fixed::chunk_fixed;
use split::chunk_split;

pub struct AiChunkNode;

#[async_trait]
impl Node for AiChunkNode {
    fn node_type(&self) -> &str {
        "ai_chunk"
    }

    fn description(&self) -> &str {
        "Split text into chunks using fixed-size, delimiter, or subtitle cue strategies"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let mode = config
            .get("mode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("fixed");
        let source_key = config
            .get("source_key")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("ai_chunk requires 'source_key' parameter"))?;
        let source_key = interpolate_ctx(source_key, ctx);
        let output_key = config
            .get("output_key")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("chunks");

        let mut output = match mode {
            "fixed" | "split" => chunk_text(mode, config, ctx, &source_key, output_key)?,
            "cues" => chunk_subtitle_cues(config, ctx, &source_key, output_key)?,
            other => anyhow::bail!(
                "ai_chunk: unsupported mode '{}' (use 'fixed', 'split', or 'cues')",
                other
            ),
        };
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

fn chunk_text(
    mode: &str,
    config: &serde_json::Value,
    ctx: &Context,
    source_key: &str,
    output_key: &str,
) -> Result<NodeOutput> {
    let text = ctx
        .get(source_key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "ai_chunk: source_key '{}' not found or not a string in context",
                source_key
            )
        })?;
    let chunks = if mode == "fixed" {
        let size = config_usize_strict(config, "size", ctx)?.unwrap_or(4096);
        if size == 0 {
            anyhow::bail!("ai_chunk: 'size' must be greater than 0 for mode 'fixed'");
        }
        let delimiters = config
            .get("delimiters")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let prefix = config_bool(config, "prefix", ctx).unwrap_or(false);
        chunk_fixed(text, size, delimiters, prefix)
    } else {
        let delimiters = config
            .get("delimiters")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("\n.?");
        let min_chars = config_usize_strict(config, "min_chars", ctx)?.unwrap_or(0);
        chunk_split(text, delimiters, min_chars)
    };
    let count = chunks.len();
    let mut output = NodeOutput::new();
    output.insert(
        output_key.to_string(),
        serde_json::Value::Array(chunks.into_iter().map(serde_json::Value::String).collect()),
    );
    output.insert(format!("{}_count", output_key), serde_json::json!(count));
    Ok(output)
}

fn chunk_subtitle_cues(
    config: &serde_json::Value,
    ctx: &Context,
    source_key: &str,
    output_key: &str,
) -> Result<NodeOutput> {
    let size = config_usize_strict(config, "size", ctx)?.unwrap_or(1200);
    if size == 0 {
        anyhow::bail!("ai_chunk: 'size' must be greater than 0 for mode 'cues'");
    }
    let cues = ctx
        .get(source_key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "ai_chunk: mode 'cues' requires 'source_key' ('{}') pointing to a cues array",
                source_key
            )
        })?;
    let segments = chunk_cues(cues, size)?;
    let texts = segments
        .iter()
        .map(|segment| {
            segment
                .get("text")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        })
        .collect();
    let count = segments.len();
    let mut output = NodeOutput::new();
    output.insert(output_key.to_string(), serde_json::Value::Array(segments));
    output.insert(
        format!("{}_texts", output_key),
        serde_json::Value::Array(texts),
    );
    output.insert(format!("{}_count", output_key), serde_json::json!(count));
    Ok(output)
}
