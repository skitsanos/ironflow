use anyhow::Result;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

/// Validate the `format` parameter — must be "text" or "markdown".
pub(super) fn validate_format<'a>(
    config: &'a serde_json::Value,
    node_name: &str,
) -> Result<&'a str> {
    let format = config
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    match format {
        "text" | "markdown" => Ok(format),
        other => anyhow::bail!(
            "{}: unsupported format '{}'. Must be 'text' or 'markdown'.",
            node_name,
            other
        ),
    }
}

/// Validate the `format` parameter for extract_word — also accepts "json".
pub(super) fn validate_word_format<'a>(
    config: &'a serde_json::Value,
    node_name: &str,
) -> Result<&'a str> {
    let format = config
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    match format {
        "text" | "markdown" | "json" => Ok(format),
        other => anyhow::bail!(
            "{}: unsupported format '{}'. Must be 'text', 'markdown', or 'json'.",
            node_name,
            other
        ),
    }
}

/// Get the file path from config — either `path` (literal) or `source_key` (from context).
pub(super) fn get_path(
    config: &serde_json::Value,
    ctx: &Context,
    node_name: &str,
) -> Result<String> {
    let has_path = config.get("path").and_then(|v| v.as_str()).is_some();
    let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

    if has_path && has_source_key {
        anyhow::bail!(
            "{} accepts either 'path' or 'source_key', not both",
            node_name
        );
    }

    if let Some(path_str) = config.get("path").and_then(|v| v.as_str()) {
        Ok(interpolate_ctx(path_str, ctx))
    } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
        let val = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        match val {
            serde_json::Value::String(s) => Ok(s.clone()),
            _ => anyhow::bail!("Context key '{}' must be a string (file path)", source_key),
        }
    } else {
        anyhow::bail!("{} requires either 'path' or 'source_key'", node_name)
    }
}
