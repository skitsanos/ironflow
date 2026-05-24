use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct JsonParseNode;

#[async_trait]
impl Node for JsonParseNode {
    fn node_type(&self) -> &str {
        "json_parse"
    }

    fn description(&self) -> &str {
        "Parse a JSON string from context into a value"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_parse requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_parse requires 'output_key'"))?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let json_str = source
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not a string", source_key))?;

        let parsed: serde_json::Value = serde_json::from_str(json_str)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), parsed);
        Ok(output)
    }
}

pub struct JsonStringifyNode;

#[async_trait]
impl Node for JsonStringifyNode {
    fn node_type(&self) -> &str {
        "json_stringify"
    }

    fn description(&self) -> &str {
        "Serialize a context value to a JSON string"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_stringify requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_stringify requires 'output_key'"))?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let json_str = serde_json::to_string(source)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(json_str));
        Ok(output)
    }
}

pub struct JsonExtractPathNode;

#[async_trait]
impl Node for JsonExtractPathNode {
    fn node_type(&self) -> &str {
        "json_extract_path"
    }

    fn description(&self) -> &str {
        "Extract a value from JSON data using a dotted path with optional array indexes"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_extract_path requires 'source_key'"))?;

        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_extract_path requires 'path'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_extract_path requires 'output_key'"))?;

        let parse_json = config
            .get("parse_json")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let required = config
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let default_value = config.get("default").cloned();

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let source = if parse_json {
            if let Some(json_text) = source.as_str() {
                serde_json::from_str(json_text).map_err(|err| {
                    anyhow::anyhow!(
                        "json_extract_path failed to parse '{}' as JSON: {}",
                        source_key,
                        err
                    )
                })?
            } else {
                source.clone()
            }
        } else {
            source.clone()
        };

        let value = if path.trim().is_empty() {
            Some(source)
        } else {
            resolve_json_path(&source, path).cloned()
        };

        let output_value = match value {
            Some(v) => v,
            None if required => anyhow::bail!("Path '{}' was not found in '{}'", path, source_key),
            None => default_value.unwrap_or(serde_json::Value::Null),
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), output_value);
        Ok(output)
    }
}

pub(super) fn resolve_json_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    let mut index = 0;
    let bytes = path.as_bytes();
    let len = bytes.len();

    while index < len {
        let segment_start = index;
        while index < len && bytes[index] != b'.' && bytes[index] != b'[' {
            index += 1;
        }

        if index > segment_start {
            let key = &path[segment_start..index];
            current = current.as_object()?.get(key)?;
        }

        if index < len && bytes[index] == b'[' {
            index += 1;

            let bracket_start = index;
            while index < len && bytes[index] != b']' {
                index += 1;
            }
            if index >= len {
                return None;
            }

            let index_text = path[bracket_start..index].trim();
            let array_index = index_text.parse::<usize>().ok()?;
            current = current.as_array()?.get(array_index)?;

            index += 1;
        }

        if index < len && bytes[index] == b'.' {
            index += 1;
        }
    }

    Some(current)
}
