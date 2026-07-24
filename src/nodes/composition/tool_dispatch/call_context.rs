use serde_json::Value;

use crate::engine::types::Context;

pub(super) fn build_child_context(mapping: &Value, parent_ctx: &Context, call: &Value) -> Context {
    let mut child_ctx = Context::new();
    child_ctx.insert("tool_call".to_string(), call.clone());
    child_ctx.insert(
        "tool_name".to_string(),
        call.get("name").cloned().unwrap_or(Value::Null),
    );
    child_ctx.insert(
        "tool_arguments".to_string(),
        call.get("arguments").cloned().unwrap_or(Value::Null),
    );
    child_ctx.insert(
        "tool_call_id".to_string(),
        call.get("id").cloned().unwrap_or(Value::Null),
    );
    child_ctx.insert(
        "tool_call_index".to_string(),
        call.get("index").cloned().unwrap_or(Value::Null),
    );

    if let Some(input) = mapping.get("input").and_then(Value::as_object) {
        for (key, spec) in input {
            child_ctx.insert(key.clone(), resolve_input_value(spec, parent_ctx, call));
        }
    }
    child_ctx
}

fn resolve_input_value(spec: &Value, parent_ctx: &Context, call: &Value) -> Value {
    match spec {
        Value::String(value) => resolve_string(value, parent_ctx, call),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| resolve_input_value(item, parent_ctx, call))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), resolve_input_value(value, parent_ctx, call)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn resolve_string(value: &str, parent_ctx: &Context, call: &Value) -> Value {
    if let Some(path) = value.strip_prefix("arguments.") {
        return resolve_path(call.get("arguments").unwrap_or(&Value::Null), path)
            .cloned()
            .unwrap_or(Value::Null);
    }
    if value == "arguments" {
        return call.get("arguments").cloned().unwrap_or(Value::Null);
    }
    if let Some(path) = value.strip_prefix("call.") {
        return resolve_path(call, path).cloned().unwrap_or(Value::Null);
    }
    if value == "call" {
        return call.clone();
    }
    if let Some(path) = value.strip_prefix("ctx.") {
        return resolve_context_path(parent_ctx, path);
    }
    match value {
        "tool_name" => call.get("name").cloned().unwrap_or(Value::Null),
        "tool_call_id" => call.get("id").cloned().unwrap_or(Value::Null),
        key => parent_ctx
            .get(key)
            .cloned()
            .unwrap_or_else(|| Value::String(value.to_string())),
    }
}

fn resolve_context_path(ctx: &Context, path: &str) -> Value {
    let mut parts = path.splitn(2, '.');
    let key = parts.next().unwrap_or_default();
    let Some(root) = ctx.get(key) else {
        return Value::Null;
    };
    parts
        .next()
        .and_then(|rest| resolve_path(root, rest))
        .cloned()
        .unwrap_or_else(|| root.clone())
}

fn resolve_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for part in path.split('.') {
        if part.is_empty() {
            return None;
        }
        current = if let Ok(index) = part.parse::<usize>() {
            current.as_array()?.get(index)?
        } else {
            current.get(part)?
        };
    }
    Some(current)
}
