use std::collections::HashSet;

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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
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

pub struct SelectFieldsNode;

#[async_trait]
impl Node for SelectFieldsNode {
    fn node_type(&self) -> &str {
        "select_fields"
    }

    fn description(&self) -> &str {
        "Select specific fields from a context object"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("select_fields requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("select_fields requires 'output_key'"))?;

        let fields = config
            .get("fields")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("select_fields requires 'fields' array"))?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let source_obj = source
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an object", source_key))?;

        let mut selected = serde_json::Map::new();
        for field in fields {
            if let Some(field_name) = field.as_str()
                && let Some(value) = source_obj.get(field_name)
            {
                selected.insert(field_name.to_string(), value.clone());
            }
        }

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Object(selected));
        Ok(output)
    }
}

pub struct RenameFieldsNode;

#[async_trait]
impl Node for RenameFieldsNode {
    fn node_type(&self) -> &str {
        "rename_fields"
    }

    fn description(&self) -> &str {
        "Rename fields in a context object"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("rename_fields requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("rename_fields requires 'output_key'"))?;

        let mapping = config
            .get("mapping")
            .and_then(|v| v.as_object())
            .ok_or_else(|| anyhow::anyhow!("rename_fields requires 'mapping' object"))?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let source_obj = source
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an object", source_key))?;

        let mut result = serde_json::Map::new();
        for (old_key, value) in source_obj {
            // Check if this key has a rename mapping
            if let Some(new_key_val) = mapping.get(old_key) {
                if let Some(new_key) = new_key_val.as_str() {
                    result.insert(new_key.to_string(), value.clone());
                } else {
                    result.insert(old_key.clone(), value.clone());
                }
            } else {
                result.insert(old_key.clone(), value.clone());
            }
        }

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Object(result));
        Ok(output)
    }
}

pub struct DataFilterNode;

#[async_trait]
impl Node for DataFilterNode {
    fn node_type(&self) -> &str {
        "data_filter"
    }

    fn description(&self) -> &str {
        "Filter array items by a condition"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_filter requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_filter requires 'output_key'"))?;

        let field = config
            .get("field")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_filter requires 'field'"))?;

        let op = config.get("op").and_then(|v| v.as_str()).ok_or_else(|| {
            anyhow::anyhow!(
                "data_filter requires 'op' (eq, neq, gt, lt, gte, lte, contains, exists)"
            )
        })?;

        let compare_value = config.get("value");

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let items = source
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))?;

        let filtered: Vec<serde_json::Value> = items
            .iter()
            .filter(|item| filter_match(item, field, op, compare_value))
            .cloned()
            .collect();

        let count = filtered.len();
        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(filtered));
        output.insert(format!("{}_count", output_key), serde_json::json!(count));
        Ok(output)
    }
}

/// Evaluate a filter condition on a single item.
fn filter_match(
    item: &serde_json::Value,
    field: &str,
    op: &str,
    compare_value: Option<&serde_json::Value>,
) -> bool {
    let field_val = item.get(field);

    match op {
        "exists" => field_val.is_some() && !field_val.unwrap().is_null(),
        "not_exists" => field_val.is_none() || field_val.unwrap().is_null(),
        _ => {
            let field_val = match field_val {
                Some(v) => v,
                None => return false,
            };
            let cmp = match compare_value {
                Some(v) => v,
                None => return false,
            };

            match op {
                "eq" => field_val == cmp,
                "neq" => field_val != cmp,
                "gt" => field_val
                    .as_f64()
                    .zip(cmp.as_f64())
                    .is_some_and(|(a, b)| a > b),
                "lt" => field_val
                    .as_f64()
                    .zip(cmp.as_f64())
                    .is_some_and(|(a, b)| a < b),
                "gte" => field_val
                    .as_f64()
                    .zip(cmp.as_f64())
                    .is_some_and(|(a, b)| a >= b),
                "lte" => field_val
                    .as_f64()
                    .zip(cmp.as_f64())
                    .is_some_and(|(a, b)| a <= b),
                "contains" => {
                    if let (Some(haystack), Some(needle)) = (field_val.as_str(), cmp.as_str()) {
                        haystack.contains(needle)
                    } else {
                        false
                    }
                }
                _ => false,
            }
        }
    }
}

pub struct DataTransformNode;

#[async_trait]
impl Node for DataTransformNode {
    fn node_type(&self) -> &str {
        "data_transform"
    }

    fn description(&self) -> &str {
        "Transform data by mapping and renaming fields"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_transform requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_transform requires 'output_key'"))?;

        let mapping = config
            .get("mapping")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                anyhow::anyhow!("data_transform requires 'mapping' object (new_name -> old_name)")
            })?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let result = match source {
            serde_json::Value::Array(arr) => {
                // Apply mapping to each item in the array
                let transformed: Vec<serde_json::Value> = arr
                    .iter()
                    .map(|item| apply_mapping(item, mapping))
                    .collect();
                serde_json::Value::Array(transformed)
            }
            serde_json::Value::Object(_) => {
                // Apply mapping to a single object
                apply_mapping(source, mapping)
            }
            _ => anyhow::bail!("Value at '{}' must be an object or array", source_key),
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), result);
        Ok(output)
    }
}

/// Apply a field mapping to a single value. Mapping is { new_name: "old_name" }.
fn apply_mapping(
    item: &serde_json::Value,
    mapping: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    for (new_name, old_name_val) in mapping {
        if let Some(old_name) = old_name_val.as_str()
            && let Some(value) = item.get(old_name)
        {
            result.insert(new_name.clone(), value.clone());
        }
    }
    serde_json::Value::Object(result)
}

pub struct BatchNode;

#[async_trait]
impl Node for BatchNode {
    fn node_type(&self) -> &str {
        "batch"
    }

    fn description(&self) -> &str {
        "Split an array into chunks of a specified size"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("batch requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("batch requires 'output_key'"))?;

        let size = config
            .get("size")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("batch requires 'size' (positive integer)"))?
            as usize;

        if size == 0 {
            anyhow::bail!("batch 'size' must be greater than 0");
        }

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let items = source
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))?;

        let batches: Vec<serde_json::Value> = items
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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("deduplicate requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("deduplicate requires 'output_key'"))?;

        let key_field = config.get("key").and_then(|v| v.as_str());

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let items = source
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))?;

        let mut seen = HashSet::new();
        let mut unique = Vec::new();

        for item in items {
            let dedup_key = match key_field {
                Some(field) => {
                    // Deduplicate by a specific field value
                    item.get(field).map(|v| v.to_string()).unwrap_or_default()
                }
                None => {
                    // Deduplicate by full JSON serialization
                    serde_json::to_string(item).unwrap_or_default()
                }
            };

            if seen.insert(dedup_key) {
                unique.push(item.clone());
            }
        }

        let original_count = items.len();
        let unique_count = unique.len();
        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(unique));
        output.insert(
            format!("{}_removed", output_key),
            serde_json::json!(original_count - unique_count),
        );
        Ok(output)
    }
}
