use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::json_equality::json_equal;

const OPERATORS: &str = "eq, neq, gt, lt, gte, lte, contains, exists, not_exists";

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
        let op = config
            .get("op")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_filter requires 'op' ({OPERATORS})"))?;
        // An unknown operator used to match nothing, which read as a filter
        // that legitimately found no items (IF-196).
        anyhow::ensure!(
            OPERATORS.split(", ").any(|known| known == op),
            "data_filter: unknown op '{op}' (expected one of {OPERATORS})"
        );

        // A comparison without `value` matched nothing for every operator,
        // which read as a filter that found no items; with `neq` defined as
        // the negation of `eq` it would match everything instead (IF-223).
        let compare_value = config.get("value");
        let presence_only = matches!(op, "exists" | "not_exists");
        anyhow::ensure!(
            presence_only || compare_value.is_some(),
            "data_filter: op '{op}' requires 'value'"
        );
        let compare_value = compare_value.unwrap_or(&serde_json::Value::Null);

        let items = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))?;
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

/// A field that is missing, or an item that is not an object, compares as
/// `null`: `eq` and `neq` then split every array in two, as `not_exists`
/// and `exists` do, and `neq` is the exact negation of `eq` (IF-223).
/// Ordering and `contains` have no result for a value of another type and
/// exclude the item.
fn filter_match(
    item: &serde_json::Value,
    field: &str,
    op: &str,
    compare_value: &serde_json::Value,
) -> bool {
    let field_value = item.get(field).unwrap_or(&serde_json::Value::Null);
    match op {
        "exists" => !field_value.is_null(),
        "not_exists" => field_value.is_null(),
        "eq" => json_equal(field_value, compare_value),
        "neq" => !json_equal(field_value, compare_value),
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
