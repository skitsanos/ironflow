use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::node_config::config_usize_strict;

pub struct BatchNode;

#[async_trait]
impl Node for BatchNode {
    fn node_type(&self) -> &str {
        "batch"
    }

    fn description(&self) -> &str {
        "Split an array into chunks of a specified size"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("batch requires 'source_key'"))?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("batch requires 'output_key'"))?;
        let size = config_usize_strict(config, "size", ctx)?
            .ok_or_else(|| anyhow::anyhow!("batch requires 'size' (positive integer)"))?;
        if size == 0 {
            anyhow::bail!("batch 'size' must be greater than 0");
        }

        let items = context_array(ctx, source_key)?;
        let batches: Vec<_> = items
            .chunks(size)
            .map(|chunk| serde_json::Value::Array(chunk.to_vec()))
            .collect();
        let batch_count = batches.len();

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(batches));
        output.insert(
            format!("{}_count", output_key),
            serde_json::json!(batch_count),
        );
        Ok(output)
    }
}

pub struct DeduplicateNode;

#[async_trait]
impl Node for DeduplicateNode {
    fn node_type(&self) -> &str {
        "deduplicate"
    }

    fn description(&self) -> &str {
        "Remove duplicate items from an array"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("deduplicate requires 'source_key'"))?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("deduplicate requires 'output_key'"))?;
        let key_field = config.get("key").and_then(|v| v.as_str());
        let items = context_array(ctx, source_key)?;

        let mut seen = HashSet::new();
        let unique: Vec<_> = items
            .iter()
            .filter(|item| seen.insert(dedup_key(item, key_field)))
            .cloned()
            .collect();
        let removed = items.len() - unique.len();

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(unique));
        output.insert(
            format!("{}_removed", output_key),
            serde_json::json!(removed),
        );
        Ok(output)
    }
}

fn context_array<'a>(ctx: &'a Context, source_key: &str) -> Result<&'a Vec<serde_json::Value>> {
    ctx.get(source_key)
        .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))
}

fn dedup_key(item: &serde_json::Value, key_field: Option<&str>) -> String {
    match key_field {
        Some(field) => item.get(field).map(ToString::to_string).unwrap_or_default(),
        None => serde_json::to_string(item).unwrap_or_default(),
    }
}
