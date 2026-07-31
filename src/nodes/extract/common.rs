use anyhow::Result;

/// Validate the `format` parameter — must be "text" or "markdown".
pub(super) fn validate_format<'a>(
    config: &'a serde_json::Value,
    node_name: &str,
) -> Result<&'a str> {
    let format = string_or(config, "format", "text", node_name)?;
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
    let format = string_or(config, "format", "text", node_name)?;
    match format {
        "text" | "markdown" | "json" => Ok(format),
        other => anyhow::bail!(
            "{}: unsupported format '{}'. Must be 'text', 'markdown', or 'json'.",
            node_name,
            other
        ),
    }
}

pub(super) fn optional_string<'a>(
    config: &'a serde_json::Value,
    key: &str,
    node_name: &str,
) -> Result<Option<&'a str>> {
    match config.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(_) => anyhow::bail!("{node_name}: '{key}' must be a string"),
    }
}

pub(super) fn string_or<'a>(
    config: &'a serde_json::Value,
    key: &str,
    default: &'a str,
    node_name: &str,
) -> Result<&'a str> {
    Ok(optional_string(config, key, node_name)?.unwrap_or(default))
}

pub(super) fn ensure_distinct_keys(node_name: &str, keys: &[(&str, &str)]) -> Result<()> {
    for (index, (left_name, left_value)) in keys.iter().enumerate() {
        for (right_name, right_value) in &keys[index + 1..] {
            if left_value == right_value {
                anyhow::bail!(
                    "{node_name}: '{left_name}' and '{right_name}' must name different context keys"
                );
            }
        }
    }
    Ok(())
}
