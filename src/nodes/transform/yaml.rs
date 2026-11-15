use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::yaml_numbers;
// Direct defaults preserve merge expansion and YAML 1.2 scalar resolution.
use noyalib as yaml;

pub struct YamlParseNode;

#[async_trait]
impl Node for YamlParseNode {
    fn node_type(&self) -> &str {
        "yaml_parse"
    }

    fn description(&self) -> &str {
        "Parse a YAML string into a JSON value"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let has_input = config.get("input").and_then(|v| v.as_str()).is_some();
        let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

        if has_input && has_source_key {
            anyhow::bail!("yaml_parse: provide either 'input' or 'source_key', not both");
        }

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("yaml_data");

        let yaml_str = if let Some(input_str) = config.get("input").and_then(|v| v.as_str()) {
            interpolate_ctx(input_str, ctx)
        } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
            let val = ctx
                .get(source_key)
                .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
            match val {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string(other)?,
            }
        } else {
            anyhow::bail!("yaml_parse requires either 'input' or 'source_key'");
        };

        let json_value = yaml_numbers::parse(&yaml_str)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), json_value);
        Ok(output)
    }
}

pub struct YamlStringifyNode;

#[async_trait]
impl Node for YamlStringifyNode {
    fn node_type(&self) -> &str {
        "yaml_stringify"
    }

    fn description(&self) -> &str {
        "Convert a JSON value from context to a YAML string"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("yaml_stringify requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("yaml");

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        // The writer refuses such an integer with a message about build
        // features; name the value instead (IF-223).
        if let Some(integer) = yaml_numbers::unwritable_integer(source) {
            anyhow::bail!(
                "yaml_stringify: integer {integer} is above the signed 64-bit range and \
                 cannot be written exactly; convert it to a string first"
            );
        }
        let yaml_str = yaml::to_string(source)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(yaml_str));
        Ok(output)
    }
}
