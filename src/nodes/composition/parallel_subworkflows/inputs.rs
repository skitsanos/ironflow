use std::sync::Arc;

use anyhow::Result;
use serde_json::{Map, Value};

use crate::engine::types::Context;

use super::super::parallel_runner::validate_child_output_key;

/// One child of the fan-out: its configuration and, in dynamic mode, where
/// its item lives in the parent context. It holds no copy of the item or of
/// the parent, so the entries of a large fan-out stay small (IF-188).
pub(in super::super) struct FlowEntry {
    pub(in super::super) config: Arc<Value>,
    item: Option<DynamicItem>,
}

struct DynamicItem {
    keys: Arc<DynamicKeys>,
    index: usize,
}

struct DynamicKeys {
    source: String,
    item: String,
    index: String,
}

impl FlowEntry {
    /// Build the child's context. Called when the child starts, which is
    /// after it holds its concurrency slot.
    pub(in super::super) fn child_context(&self, parent: &Context) -> Context {
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
        if let Some(item) = &self.item {
            let value = parent
                .get(&item.keys.source)
                .and_then(Value::as_array)
                .and_then(|source| source.get(item.index))
                .cloned()
                .unwrap_or(Value::Null);
            context.insert(item.keys.item.clone(), value);
            context.insert(item.keys.index.clone(), Value::from(item.index + 1));
        }
        context
    }
}

fn ensure_within_ceiling(count: usize) -> Result<()> {
    let limit = crate::util::limits::max_parallel_subworkflows();
    if count > limit {
        anyhow::bail!(
            "parallel_subworkflows: {count} child flows exceeds the limit of {limit} \
             (IRONFLOW_MAX_PARALLEL_SUBWORKFLOWS)"
        );
    }
    Ok(())
}

pub(super) fn resolve_flow_entries(config: &Value, ctx: &Context) -> Result<Vec<FlowEntry>> {
    if let Some(flows) = config.get("flows") {
        let flows = flows.as_array().ok_or_else(|| {
            anyhow::anyhow!("parallel_subworkflows requires 'flows' array parameter")
        })?;
        if flows.is_empty() {
            anyhow::bail!("parallel_subworkflows: 'flows' array must not be empty");
        }
        ensure_within_ceiling(flows.len())?;
        for entry in flows {
            validate_child_output_key(entry.get("output_key").and_then(Value::as_str))?;
        }
        return Ok(flows
            .iter()
            .map(|config| FlowEntry {
                config: Arc::new(config.clone()),
                item: None,
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
    ensure_within_ceiling(source.len())?;
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
    let child_config = Arc::new(Value::Object(child_config));
    let keys = Arc::new(DynamicKeys {
        source: source_key.to_string(),
        item: item_key.to_string(),
        index: index_key.to_string(),
    });

    Ok((0..source.len())
        .map(|index| FlowEntry {
            config: child_config.clone(),
            item: Some(DynamicItem {
                keys: keys.clone(),
                index,
            }),
        })
        .collect())
}

#[cfg(test)]
mod tests;
