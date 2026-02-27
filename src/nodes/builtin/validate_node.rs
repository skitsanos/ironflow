use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

fn validate_against_schema(
    data: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(bool, Vec<String>)> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|e| anyhow::anyhow!("Invalid JSON schema: {}", e))?;

    let errors: Vec<String> = validator
        .iter_errors(data)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();

    Ok((errors.is_empty(), errors))
}

fn build_validation_output(success: bool, errors: Vec<String>) -> NodeOutput {
    let mut output = NodeOutput::new();
    output.insert(
        "validation_success".to_string(),
        serde_json::Value::Bool(success),
    );
    output.insert(
        "validation_errors".to_string(),
        serde_json::Value::Array(
            errors
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );
    output
}

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

        let (success, errors) = validate_against_schema(data, schema)?;
        let output = build_validation_output(success, errors);

        if !success {
            let details = output
                .get("validation_errors")
                .and_then(|v| v.as_array())
                .map(|errors| {
                    errors
                        .iter()
                        .filter_map(|e| e.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .filter(|details| !details.is_empty())
                .unwrap_or_else(|| "Schema validation failed".to_string());

            anyhow::bail!("Schema validation failed: {}", details);
        }

        Ok(output)
    }
}

pub struct JsonValidateNode;

#[async_trait]
impl Node for JsonValidateNode {
    fn node_type(&self) -> &str {
        "json_validate"
    }

    fn description(&self) -> &str {
        "Validate JSON text against a JSON Schema"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_validate requires 'source_key'"))?;

        let schema = config
            .get("schema")
            .ok_or_else(|| anyhow::anyhow!("json_validate requires 'schema'"))?;

        let raw = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let data = if let Some(raw_text) = raw.as_str() {
            serde_json::from_str(raw_text)
                .map_err(|e| anyhow::anyhow!("Invalid JSON string in '{}': {}", source_key, e))?
        } else {
            raw.clone()
        };

        let (success, errors) = validate_against_schema(&data, schema)?;
        let output = build_validation_output(success, errors);

        if !success {
            let details = output
                .get("validation_errors")
                .and_then(|v| v.as_array())
                .map(|errors| {
                    errors
                        .iter()
                        .filter_map(|e| e.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .filter(|details| !details.is_empty())
                .unwrap_or_else(|| "Schema validation failed".to_string());

            anyhow::bail!("Schema validation failed: {}", details);
        }

        Ok(output)
    }
}
