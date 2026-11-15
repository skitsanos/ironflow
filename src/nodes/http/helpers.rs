use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_value;
use anyhow::Result;

/// Recursively interpolate context templates in all JSON string values.
pub(super) fn interpolate_json_value(
    value: &serde_json::Value,
    ctx: &Context,
) -> serde_json::Value {
    interpolate_value(value, ctx)
}

/// Simple percent-encoding for form body values.
pub(super) fn percent_encode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

pub(super) fn body_value_to_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(super) fn build_form_body(body: &serde_json::Value) -> Result<String> {
    let object = body
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("body_type='form' requires 'body' to be an object"))?;

    let mut pairs = Vec::with_capacity(object.len());
    for (key, value) in object {
        pairs.push(format!(
            "{}={}",
            percent_encode(key),
            percent_encode(&body_value_to_text(value))
        ));
    }
    Ok(pairs.join("&"))
}
