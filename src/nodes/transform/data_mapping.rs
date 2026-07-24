use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct DataTransformNode;

#[async_trait]
impl Node for DataTransformNode {
    fn node_type(&self) -> &str {
        "data_transform"
    }

    fn description(&self) -> &str {
        "Transform data by mapping and renaming fields"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
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
            serde_json::Value::Array(items) => serde_json::Value::Array(
                items
                    .iter()
                    .map(|item| apply_mapping(item, mapping))
                    .collect(),
            ),
            serde_json::Value::Object(_) => apply_mapping(source, mapping),
            _ => anyhow::bail!("Value at '{}' must be an object or array", source_key),
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), result);
        Ok(output)
    }
}

fn apply_mapping(
    item: &serde_json::Value,
    mapping: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    serde_json::Value::Object(
        mapping
            .iter()
            .filter_map(|(new_name, old_name)| {
                item.get(old_name.as_str()?)
                    .map(|value| (new_name.clone(), value.clone()))
            })
            .collect(),
    )
}
