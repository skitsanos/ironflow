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
