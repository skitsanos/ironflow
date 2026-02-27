use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct AiChunkMergeNode;

/// Count tokens using whitespace split (simple approximation).
fn count_tokens(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Greedily merge consecutive chunks while total tokens <= chunk_size.
fn merge_chunks(chunks: &[String], chunk_size: usize) -> Vec<String> {
    if chunks.is_empty() {
        return vec![];
    }

    let token_counts: Vec<usize> = chunks.iter().map(|c| count_tokens(c)).collect();

    let mut merged = Vec::new();
    let mut group_start = 0;
    let mut group_tokens = 0;

    for (i, &tokens) in token_counts.iter().enumerate() {
        if group_tokens + tokens > chunk_size && i > group_start {
            // Flush current group
            let joined = chunks[group_start..i].join("\n\n");
            merged.push(joined);
            group_start = i;
            group_tokens = 0;
        }
        group_tokens += tokens;
    }

    // Flush remaining group
    if group_start < chunks.len() {
        let joined = chunks[group_start..].join("\n\n");
        merged.push(joined);
    }

    merged
}

#[async_trait]
impl Node for AiChunkMergeNode {
    fn node_type(&self) -> &str {
        "ai_chunk_merge"
    }

    fn description(&self) -> &str {
        "Merge small text chunks into token-budget groups"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("ai_chunk_merge requires 'source_key' parameter"))?;
        let source_key = interpolate_ctx(source_key, &ctx);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("merged")
            .to_string();

        let chunk_size = config
            .get("chunk_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(512) as usize;

        // Get source chunks from context
        let source_value = ctx.get(&source_key).ok_or_else(|| {
            anyhow::anyhow!(
                "ai_chunk_merge: source_key '{}' not found in context",
                source_key
            )
        })?;

        let chunks: Vec<String> = match source_value {
            serde_json::Value::Array(arr) => arr
                .iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect(),
            _ => {
                anyhow::bail!(
                    "ai_chunk_merge: source_key '{}' must be an array of strings",
                    source_key
                );
            }
        };

        let merged = merge_chunks(&chunks, chunk_size);
        let count = merged.len();
        let merged_json: Vec<serde_json::Value> =
            merged.into_iter().map(serde_json::Value::String).collect();

        let mut output = NodeOutput::new();
        output.insert(output_key.clone(), serde_json::Value::Array(merged_json));
        output.insert(format!("{}_count", output_key), serde_json::json!(count));
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );

        Ok(output)
    }
}
