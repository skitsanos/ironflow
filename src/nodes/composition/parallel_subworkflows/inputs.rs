use anyhow::Result;
use serde_json::{Map, Value};

use crate::engine::types::Context;

use super::super::parallel_runner::validate_child_output_key;

pub(super) struct FlowEntry {
    pub(super) config: Value,
    literal_input: Context,
}

impl FlowEntry {
    pub(super) fn into_parts(self, parent: &Context) -> (Value, Context) {
        let mut context = match self.config.get("input").and_then(Value::as_object) {
            Some(input) => input
                .iter()
                .map(|(key, source)| {
                    let value = source
                        .as_str()
                        .and_then(|parent_key| parent.get(parent_key))
                        .unwrap_or(source)
                        .clone();
                    (key.clone(), value)
                })
                .collect(),
            None => parent.clone(),
        };
        // Runtime item/index values bypass reference lookup entirely.
        context.extend(self.literal_input);
        (self.config, context)
    }
}

pub(super) fn resolve_flow_entries(config: &Value, ctx: &Context) -> Result<Vec<FlowEntry>> {
    if let Some(flows) = config.get("flows") {
        let flows = flows.as_array().ok_or_else(|| {
            anyhow::anyhow!("parallel_subworkflows requires 'flows' array parameter")
        })?;
        if flows.is_empty() {
            anyhow::bail!("parallel_subworkflows: 'flows' array must not be empty");
        }
        for entry in flows {
            validate_child_output_key(entry.get("output_key").and_then(Value::as_str))?;
        }
        return Ok(flows
            .iter()
            .map(|config| FlowEntry {
                config: config.clone(),
                literal_input: Context::new(),
            })
            .collect());
    }

    if config.get("flow").is_some() || config.get("source_key").is_some() {
        return build_dynamic_flow_entries(config, ctx);
    }

    anyhow::bail!(
        "parallel_subworkflows requires either 'flows' array or dynamic 'flow' + 'source_key'"
    )
}

fn build_dynamic_flow_entries(config: &Value, ctx: &Context) -> Result<Vec<FlowEntry>> {
    let flow_file = config.get("flow").and_then(Value::as_str).ok_or_else(|| {
        anyhow::anyhow!(
            "parallel_subworkflows dynamic mode requires 'flow' when 'flows' is not provided"
        )
    })?;
    let source_key = config
        .get("source_key")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "parallel_subworkflows dynamic mode requires 'source_key' when 'flows' is not provided"
            )
        })?;
    let source = ctx
        .get(source_key)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "parallel_subworkflows: source_key '{}' not found or not an array",
                source_key
            )
        })?;
    let item_key = config
        .get("item_key")
        .and_then(Value::as_str)
        .unwrap_or("item");
    let index_key = config
        .get("index_key")
        .and_then(Value::as_str)
        .unwrap_or("index");
    let child_output_key = config.get("child_output_key").and_then(Value::as_str);
    validate_child_output_key(child_output_key)?;

    let mut child_config = Map::new();
    child_config.insert("flow".to_string(), Value::String(flow_file.to_string()));
    if let Some(key) = child_output_key {
        child_config.insert("output_key".to_string(), Value::String(key.to_string()));
    }
    let mut input = config
        .get("input")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    input.remove(item_key);
    input.remove(index_key);
    // An explicit empty mapping keeps dynamic fan-out from inheriting the parent.
    child_config.insert("input".to_string(), Value::Object(input));
    let child_config = Value::Object(child_config);

    Ok(source
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let mut literal_input = Context::new();
            literal_input.insert(item_key.to_string(), item.clone());
            literal_input.insert(index_key.to_string(), Value::from(index + 1));
            FlowEntry {
                config: child_config.clone(),
                literal_input,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn literal_payload_stays_out_of_config_and_moves_into_child_context() {
        let parent = Context::from([
            ("jobs".into(), json!(["x".repeat(3 * 1024 * 1024)])),
            ("reference".into(), json!({"mapped": true})),
        ]);
        let config = json!({"flow": "child.lua", "source_key": "jobs", "input": {
            "item": "reference", "index": "reference", "mapped": "reference"
        }});
        let entry = resolve_flow_entries(&config, &parent)
            .unwrap()
            .pop()
            .unwrap();
        assert!(entry.config["input"].get("item").is_none());
        assert!(entry.config["input"].get("index").is_none());
        let pointer = entry.literal_input["item"].as_str().unwrap().as_ptr();
        let (_, child) = entry.into_parts(&parent);
        assert_eq!(child["item"].as_str().unwrap().as_ptr(), pointer);
        assert_eq!(child["index"], 1);
        assert_eq!(child["mapped"], parent["reference"]);
    }

    #[test]
    fn user_config_cannot_populate_the_internal_literal_input_field() {
        let parent = Context::from([("customer".into(), json!("mapped-value"))]);
        let config = json!({"flows": [{
            "flow": "child.lua", "input": {"item": "customer"},
            "literal_input": {"item": "not-an-input-option"}
        }]});
        let entry = resolve_flow_entries(&config, &parent)
            .unwrap()
            .pop()
            .unwrap();
        let (_, child) = entry.into_parts(&parent);
        assert_eq!(child["item"], "mapped-value");
    }
}
