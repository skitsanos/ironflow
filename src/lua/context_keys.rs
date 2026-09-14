use std::collections::BTreeSet;

use anyhow::{Result, bail};
use serde_json::Value;

use crate::engine::types::Context;

/// Validate an optional projection without interpreting keys as paths or templates.
pub(crate) fn parse_context_keys(config: &Value) -> Result<Option<BTreeSet<&str>>> {
    let Some(value) = config.get("context_keys") else {
        return Ok(None);
    };
    let Some(values) = value.as_array() else {
        bail!("context_keys must be an array of strings");
    };
    let mut keys = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let Some(key) = value.as_str() else {
            bail!("context_keys[{index}] must be a string");
        };
        keys.insert(key);
    }
    Ok(Some(keys))
}

/// Clone only selected values before handing a context snapshot to a blocking task.
pub(crate) fn project_context(config: &Value, ctx: &Context) -> Result<Context> {
    match parse_context_keys(config)? {
        None => Ok(ctx.clone()),
        Some(keys) => Ok(keys
            .into_iter()
            .filter_map(|key| ctx.get(key).map(|value| (key.to_owned(), value.clone())))
            .collect()),
    }
}
