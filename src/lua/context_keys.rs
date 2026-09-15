use std::collections::BTreeSet;

use anyhow::{Result, bail};
use serde_json::Value;

use crate::engine::types::Context;

/// Shared wording for a projection declared on a node that cannot honor it.
pub(crate) const CONTEXT_KEYS_UNSUPPORTED: &str =
    "context_keys is only supported for code nodes and function handlers";

/// Engine-reserved context keys start with `_` (`_error_*`, `_flow_dir`,
/// `_schedule`, invocation overlays). They are never user data, so a
/// projection must not hide them.
pub(crate) fn is_engine_reserved_key(key: &str) -> bool {
    key.starts_with('_')
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
/// The allowlist governs user data keys only. Engine-reserved keys (any key
/// starting with `_`, such as the `_error_*` recovery overlay or `_flow_dir`)
/// always pass through, so a projected `on_error` handler still receives its
/// failure diagnostics. Listing a reserved key explicitly is harmless.
pub(crate) fn project_context(config: &Value, ctx: &Context) -> Result<Context> {
    match parse_context_keys(config)? {
        None => Ok(ctx.clone()),
        Some(keys) => Ok(ctx
            .iter()
            .filter(|(key, _)| is_engine_reserved_key(key) || keys.contains(key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()),
    }
}
