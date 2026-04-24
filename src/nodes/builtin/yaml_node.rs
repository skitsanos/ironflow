use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

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

        let yaml_value: serde_yml::Value = serde_yml::from_str(&yaml_str)?;
        let json_value = yaml_to_json(yaml_value);

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

        let yaml_str = serde_yml::to_string(source)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(yaml_str));
        Ok(output)
    }
}

/// Convert a serde_yml::Value into a serde_json::Value.
fn yaml_to_json(yaml: serde_yml::Value) -> serde_json::Value {
    match yaml {
        serde_yml::Value::Null => serde_json::Value::Null,
        serde_yml::Value::Bool(b) => serde_json::Value::Bool(b),
        serde_yml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                serde_json::Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        }
        serde_yml::Value::String(s) => serde_json::Value::String(s),
        serde_yml::Value::Sequence(seq) => {
            serde_json::Value::Array(seq.into_iter().map(yaml_to_json).collect())
        }
        serde_yml::Value::Mapping(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        serde_yml::Value::String(s) => s,
                        serde_yml::Value::Number(n) => n.to_string(),
                        serde_yml::Value::Bool(b) => b.to_string(),
                        _ => return None,
                    };
                    Some((key, yaml_to_json(v)))
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        serde_yml::Value::Tagged(tagged) => yaml_to_json(tagged.value),
    }
}
