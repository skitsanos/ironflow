use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct SelectFieldsNode;

#[async_trait]
impl Node for SelectFieldsNode {
    fn node_type(&self) -> &str {
        "select_fields"
    }

    fn description(&self) -> &str {
        "Select specific fields from a context object"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = required_string(config, "source_key", "select_fields")?;
        let output_key = required_string(config, "output_key", "select_fields")?;
        let fields = config
            .get("fields")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("select_fields requires 'fields' array"))?;
        let source = source_object(ctx, source_key)?;

        let selected = fields
            .iter()
            .filter_map(|field| {
                let field_name = field.as_str()?;
                source
                    .get(field_name)
                    .map(|value| (field_name.to_string(), value.clone()))
            })
            .collect();

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

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = required_string(config, "source_key", "rename_fields")?;
        let output_key = required_string(config, "output_key", "rename_fields")?;
        let mapping = config
            .get("mapping")
            .and_then(|v| v.as_object())
            .ok_or_else(|| anyhow::anyhow!("rename_fields requires 'mapping' object"))?;
        let source = source_object(ctx, source_key)?;

        let renamed = source
            .iter()
            .map(|(old_key, value)| {
                let new_key = mapping
                    .get(old_key)
                    .and_then(|candidate| candidate.as_str())
                    .unwrap_or(old_key);
                (new_key.to_string(), value.clone())
            })
            .collect();

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Object(renamed));
        Ok(output)
    }
}

fn required_string<'a>(
    config: &'a serde_json::Value,
    key: &str,
    node_type: &str,
) -> Result<&'a str> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("{} requires '{}'", node_type, key))
}

fn source_object<'a>(
    ctx: &'a Context,
    source_key: &str,
) -> Result<&'a serde_json::Map<String, serde_json::Value>> {
    ctx.get(source_key)
        .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an object", source_key))
}
