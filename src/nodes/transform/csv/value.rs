use anyhow::Result;

pub(super) fn parse_csv_single_byte(
    config: &serde_json::Value,
    key: &str,
    default: u8,
) -> Result<u8> {
    let Some(value) = config.get(key).and_then(|value| value.as_str()) else {
        return Ok(default);
    };
    match value {
        "\\t" => return Ok(b'\t'),
        "\\n" => return Ok(b'\n'),
        "\\r" => return Ok(b'\r'),
        _ => {}
    }
    let bytes = value.as_bytes();
    if bytes.len() != 1 {
        anyhow::bail!("{} must be a single-byte character", key);
    }
    Ok(bytes[0])
}

pub(super) fn csv_value_from_str(value: &str, infer_types: bool) -> serde_json::Value {
    if !infer_types {
        return serde_json::Value::String(value.to_string());
    }
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("true") {
        return serde_json::Value::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return serde_json::Value::Bool(false);
    }
    if trimmed.is_empty() {
        return serde_json::Value::String(String::new());
    }
    if let Ok(value) = trimmed.parse::<i64>() {
        return serde_json::json!(value);
    }
    if let Ok(value) = trimmed.parse::<f64>() {
        return serde_json::json!(value);
    }
    serde_json::Value::String(trimmed.to_string())
}

pub(super) fn csv_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => value.clone(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}
