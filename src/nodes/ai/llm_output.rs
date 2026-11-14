//! Turns a successful provider response into the `llm` node output.

use anyhow::Result;
use serde_json::Value;

use crate::engine::types::NodeOutput;

use super::llm_providers::LlmMode;
use super::llm_response::{
    extract_chat_reply, extract_chat_tool_calls, extract_responses_reply,
    extract_responses_tool_calls, extract_tool_call_names, normalize_tool_calls,
};

pub(super) struct ReplyMeta<'a> {
    pub(super) output_key: &'a str,
    pub(super) model: String,
    pub(super) provider: &'a str,
    pub(super) mode: LlmMode,
    pub(super) status: u16,
}

/// Builds the node output, or fails when the response carries no usable
/// reply: neither non-empty text nor a tool call.
pub(super) fn build(parsed: Value, meta: ReplyMeta<'_>) -> Result<NodeOutput> {
    let (reply, tool_calls) = match meta.mode {
        LlmMode::Chat => (
            extract_chat_reply(&parsed),
            extract_chat_tool_calls(&parsed),
        ),
        LlmMode::Responses => (
            extract_responses_reply(&parsed),
            extract_responses_tool_calls(&parsed),
        ),
    };
    let text = match reply {
        Some(reply) => reply,
        None if !tool_calls.is_empty() => "<tool call requested>".to_string(),
        // The body is provider-controlled and stays out of the error.
        None => anyhow::bail!(
            "llm: {} returned HTTP {} with no usable reply (no text and no tool calls{})",
            meta.provider,
            meta.status,
            stop_detail(&parsed, meta.mode)
        ),
    };
    let key = |suffix: &str| format!("{}_{suffix}", meta.output_key);

    let mut output = NodeOutput::new();
    output.insert(key("model"), Value::String(meta.model));
    output.insert(key("provider"), Value::String(meta.provider.to_string()));
    output.insert(key("mode"), Value::String(meta.mode.as_str().to_string()));
    output.insert(key("status"), Value::Number(meta.status.into()));
    output.insert(key("success"), Value::Bool(true));
    if let Some(usage) = parsed.get("usage") {
        output.insert(key("usage"), usage.clone());
    }
    output.insert(key("raw"), parsed);
    output.insert(key("text"), Value::String(text));
    output.insert(key("tool_call_needed"), Value::Bool(!tool_calls.is_empty()));
    output.insert(
        key("tool_call_names"),
        Value::Array(
            extract_tool_call_names(&tool_calls)
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    output.insert(
        key("tool_calls_normalized"),
        Value::Array(normalize_tool_calls(&tool_calls)),
    );
    output.insert(key("tool_calls"), Value::Array(tool_calls));

    Ok(output)
}

/// Why the provider stopped, when it said so in a short plain token.
fn stop_detail(parsed: &Value, mode: LlmMode) -> String {
    let (label, value) = match mode {
        LlmMode::Chat => (
            "finish_reason",
            parsed
                .get("choices")
                .and_then(|choices| choices.get(0))
                .and_then(|choice| choice.get("finish_reason")),
        ),
        LlmMode::Responses => ("status", parsed.get("status")),
    };
    match value.and_then(Value::as_str) {
        Some(token)
            if !token.is_empty()
                && token.len() <= 40
                && token
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-') =>
        {
            format!("; {label} '{token}'")
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn meta(mode: LlmMode) -> ReplyMeta<'static> {
        ReplyMeta {
            output_key: "llm",
            model: "fixture".to_string(),
            provider: "custom",
            mode,
            status: 200,
        }
    }

    #[test]
    fn stop_detail_reports_only_short_plain_tokens() {
        let chat = |reason: Value| json!({"choices": [{"finish_reason": reason}]});
        assert_eq!(
            stop_detail(&chat(json!("content_filter")), LlmMode::Chat),
            "; finish_reason 'content_filter'"
        );
        for hostile in [
            json!("two words"),
            json!("x".repeat(41)),
            json!(7),
            json!(""),
        ] {
            assert_eq!(stop_detail(&chat(hostile), LlmMode::Chat), "");
        }
        assert_eq!(
            stop_detail(&json!({"status": "incomplete"}), LlmMode::Responses),
            "; status 'incomplete'"
        );
    }

    #[test]
    fn untyped_gateway_items_still_yield_text_and_typed_items_do_not() {
        let untyped = json!({"output": [{"answer": "from a gateway"}]});
        let output = build(untyped, meta(LlmMode::Responses)).unwrap();
        assert_eq!(output["llm_text"], "from a gateway");

        let typed = json!({"output": [{"type": "web_search_call", "id": "ws_fixture"}]});
        let error = build(typed, meta(LlmMode::Responses)).unwrap_err();
        assert!(!error.to_string().contains("ws_fixture"));
    }

    #[test]
    fn usage_and_raw_response_are_published() {
        let parsed = json!({"choices": [{"message": {"content": "hi"}}], "usage": {"total": 3}});
        let output = build(parsed.clone(), meta(LlmMode::Chat)).unwrap();
        assert_eq!(output["llm_raw"], parsed);
        assert_eq!(output["llm_usage"], json!({"total": 3}));
        assert_eq!(output["llm_tool_calls"], json!([]));
    }
}
