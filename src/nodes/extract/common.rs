use anyhow::Result;

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
