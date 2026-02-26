use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct ValidateSchemaNode;

#[async_trait]
impl Node for ValidateSchemaNode {
    fn node_type(&self) -> &str {
        "validate_schema"
    }

    fn description(&self) -> &str {
        "Validate context data against a JSON Schema"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("validate_schema requires 'source_key'"))?;

        let schema = config
            .get("schema")
            .ok_or_else(|| anyhow::anyhow!("validate_schema requires 'schema'"))?;

        let data = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let validator = jsonschema::validator_for(schema)
            .map_err(|e| anyhow::anyhow!("Invalid JSON schema: {}", e))?;

        let errors: Vec<String> = validator
            .iter_errors(data)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();

        let success = errors.is_empty();

        let mut output = NodeOutput::new();
        output.insert(
            "validation_success".to_string(),
            serde_json::Value::Bool(success),
        );
        output.insert(
            "validation_errors".to_string(),
            serde_json::Value::Array(
                errors
                    .iter()
                    .map(|e| serde_json::Value::String(e.clone()))
                    .collect(),
            ),
        );

        if !success {
            anyhow::bail!("Schema validation failed: {}", errors.join("; "));
        }

        Ok(output)
    }
}
