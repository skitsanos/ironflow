use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct DataFilterNode;

#[async_trait]
impl Node for DataFilterNode {
    fn node_type(&self) -> &str {
        "data_filter"
    }

    fn description(&self) -> &str {
        "Filter array items by a condition"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
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

        let items = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))?;
        let compare_value = config.get("value");
        let filtered: Vec<_> = items
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

fn filter_match(
    item: &serde_json::Value,
    field: &str,
    op: &str,
    compare_value: Option<&serde_json::Value>,
) -> bool {
    let field_value = item.get(field);
    match op {
        "exists" => field_value.is_some_and(|value| !value.is_null()),
        "not_exists" => field_value.is_none_or(serde_json::Value::is_null),
        _ => compare(field_value, op, compare_value),
    }
}

fn compare(
    field_value: Option<&serde_json::Value>,
    op: &str,
    compare_value: Option<&serde_json::Value>,
) -> bool {
    let Some((field_value, compare_value)) = field_value.zip(compare_value) else {
        return false;
    };
    match op {
        "eq" => field_value == compare_value,
        "neq" => field_value != compare_value,
        "gt" => numeric_pair(field_value, compare_value).is_some_and(|(a, b)| a > b),
        "lt" => numeric_pair(field_value, compare_value).is_some_and(|(a, b)| a < b),
        "gte" => numeric_pair(field_value, compare_value).is_some_and(|(a, b)| a >= b),
        "lte" => numeric_pair(field_value, compare_value).is_some_and(|(a, b)| a <= b),
        "contains" => field_value
            .as_str()
            .zip(compare_value.as_str())
            .is_some_and(|(haystack, needle)| haystack.contains(needle)),
        _ => false,
    }
}

fn numeric_pair(left: &serde_json::Value, right: &serde_json::Value) -> Option<(f64, f64)> {
    left.as_f64().zip(right.as_f64())
}
