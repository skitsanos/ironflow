use std::collections::BTreeSet;

use anyhow::{Result, bail};
use serde_json::Value;

use crate::engine::types::Context;

/// Shared wording for a projection declared on a node that cannot honor it.
pub(crate) const CONTEXT_KEYS_UNSUPPORTED: &str =
    "context_keys is only supported for code nodes and function handlers";

/// Engine diagnostics a projection always keeps: the scalar recovery overlay
/// and the flow directory, which a recovery handler or a relative-path lookup
/// depends on and which are too small to matter for the conversion budget.
/// Bulky engine payloads (`_error_output`, `_headers`, `_webhook`, ...) stay
/// behind the allowlist like user data, otherwise a projection could not
/// exclude them and would defeat its purpose.
pub(crate) const ALWAYS_PROJECTED_KEYS: [&str; 4] = [
    "_error_message",
    "_error_step",
    "_error_node_type",
    "_flow_dir",
];

pub(crate) fn is_always_projected(key: &str) -> bool {
    ALWAYS_PROJECTED_KEYS.contains(&key)
}

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
///
/// The allowlist governs every key except the small engine diagnostics in
/// [`ALWAYS_PROJECTED_KEYS`], so a projected `on_error` handler still receives
/// `_error_message`, `_error_step` and `_error_node_type` while a bulky
/// `_error_output` or invocation overlay must be listed to be converted.
/// Listing an always-projected key explicitly is harmless.
pub(crate) fn project_context(config: &Value, ctx: &Context) -> Result<Context> {
    match parse_context_keys(config)? {
        None => Ok(ctx.clone()),
        Some(keys) => Ok(ctx
            .iter()
            .filter(|(key, _)| is_always_projected(key) || keys.contains(key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()),
    }
}
