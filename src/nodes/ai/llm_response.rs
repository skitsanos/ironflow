use serde_json::Value;

pub(super) fn extract_text(value: &serde_json::Value, out: &mut String) {
    match value {
        Value::String(s) if !s.is_empty() => {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(s);
        }
        Value::Array(items) => {
            for item in items {
                extract_text(item, out);
            }
        }
        Value::Object(map) => {
            if let Some(text) = map
                .get("text")
                .or_else(|| map.get("content"))
                .or_else(|| map.get("message").and_then(|m| m.get("content")))
            {
                extract_text(text, out);
                return;
            }

            for value in map.values() {
                extract_text(value, out);
            }
        }
        _ => {}
    }
}

pub(super) fn extract_chat_tool_calls(data: &serde_json::Value) -> Vec<serde_json::Value> {
    let Some(choices) = data.get("choices").and_then(Value::as_array) else {
        return Vec::new();
    };
    let Some(first_choice) = choices.first() else {
        return Vec::new();
    };
    let Some(message) = first_choice
        .get("message")
        .or_else(|| first_choice.get("delta"))
    else {
        return Vec::new();
    };

    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|tool_calls| {
            tool_calls
                .iter()
                .filter_map(|call| {
                    if call.is_object() {
                        Some(call.clone())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn extract_tool_call_names(tool_calls: &[serde_json::Value]) -> Vec<String> {
    tool_calls
        .iter()
        .filter_map(|call| {
            call.get("function")
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .or_else(|| call.get("name").and_then(Value::as_str))
                .map(str::to_string)
        })
        .collect()
}

pub(crate) fn normalize_tool_calls(tool_calls: &[serde_json::Value]) -> Vec<serde_json::Value> {
    tool_calls
        .iter()
        .enumerate()
        .filter_map(|(idx, call)| normalize_tool_call(call, idx))
        .collect()
}

fn normalize_tool_call(call: &serde_json::Value, idx: usize) -> Option<serde_json::Value> {
    if !call.is_object() {
        return None;
    }

    let function = call.get("function");
    let name = function
        .and_then(|f| f.get("name"))
        .and_then(Value::as_str)
        .or_else(|| call.get("name").and_then(Value::as_str))
        .unwrap_or("unknown");
    let raw_arguments = function
        .and_then(|f| f.get("arguments"))
        .or_else(|| call.get("arguments"));
    let raw_arguments_string = match raw_arguments {
        Some(Value::String(s)) => s.clone(),
        Some(value) => value.to_string(),
        None => "{}".to_string(),
    };
    let parsed_arguments = serde_json::from_str::<Value>(&raw_arguments_string)
        .unwrap_or_else(|_| Value::String(raw_arguments_string.clone()));
    let id = call
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("call_{}", idx + 1));
    let call_type = call
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("function");

    Some(serde_json::json!({
        "id": id,
        "index": idx,
        "type": call_type,
        "name": name,
        "arguments": parsed_arguments,
        "raw_arguments": raw_arguments_string,
        "raw_call": call
    }))
}

pub(super) fn extract_chat_reply(data: &serde_json::Value) -> Option<String> {
    let choices = data.get("choices")?.as_array()?;
    let first = choices.first()?;
    let msg = first.get("message");
    let mut out = String::new();

    if let Some(content) = msg.and_then(|m| m.get("content")) {
        extract_text(content, &mut out);
    }

    if out.is_empty()
        && let Some(text) = first.get("text").and_then(|v| v.as_str())
    {
        out.push_str(text);
    }

    if out.is_empty()
        && let Some(content) = first.get("content")
    {
        extract_text(content, &mut out);
    }

    if out.is_empty() { None } else { Some(out) }
}

pub(super) fn extract_responses_reply(data: &serde_json::Value) -> Option<String> {
    let mut out = String::new();

    if let Some(output_text) = data.get("output_text").and_then(|v| v.as_str()) {
        out.push_str(output_text);
    }

    if out.is_empty()
        && let Some(output) = data.get("output").and_then(|v| v.as_array())
    {
        for item in output {
            if let Some(content) = item.get("content") {
                extract_text(content, &mut out);
            }
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(text);
            }
        }
    }

    if out.is_empty()
        && let Some(output) = data.get("output").and_then(|v| v.as_array())
        && let Some(first) = output.first()
    {
        extract_text(first, &mut out);
    }

    if out.is_empty() { None } else { Some(out) }
}
