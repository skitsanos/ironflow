use anyhow::Result;
use serde_json::{Map, Value, json};

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;
use crate::util::node_config::{config_f64, config_u64};

use super::config::{LlmMode, interpolate_json_value};

pub(crate) fn resolve_messages(config: &Value, ctx: &Context) -> Result<Option<Vec<Value>>> {
    let inline = config.get("messages");
    let messages_key = match config.get("messages_key") {
        None => None,
        Some(Value::String(value)) if !value.is_empty() => Some(interpolate_ctx(value, ctx)),
        Some(_) => anyhow::bail!("llm: 'messages_key' must be a non-empty string"),
    };
    if inline.is_some() && messages_key.is_some() {
        anyhow::bail!("llm: provide either 'messages' or 'messages_key', not both");
    }
    let messages = match (inline, messages_key) {
        (Some(messages), None) => interpolate_json_value(messages, ctx),
        (None, Some(key)) => ctx
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("llm: messages_key '{key}' not found in context"))?,
        (None, None) => return Ok(None),
        (Some(_), Some(_)) => unreachable!("mutual exclusion checked above"),
    };
    let Value::Array(messages) = messages else {
        anyhow::bail!("llm: messages must be an array");
    };

    Ok((!messages.is_empty()).then_some(messages))
}

pub(crate) fn resolve_prompt(config: &Value, ctx: &Context) -> Result<String> {
    if let Some(prompt) = config.get("prompt").and_then(Value::as_str) {
        return Ok(interpolate_ctx(prompt, ctx));
    }

    if let Some(input_key) = config.get("input_key").and_then(Value::as_str) {
        let input_key = interpolate_ctx(input_key, ctx);
        return ctx
            .get(&input_key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow::anyhow!("llm: input_key '{}' not found or not a string", input_key)
            });
    }

    anyhow::bail!("llm: either 'prompt', 'input_key', 'messages', or 'messages_key' is required")
}

pub(crate) struct LlmBodyInput<'a> {
    pub(crate) mode: LlmMode,
    pub(crate) model: &'a str,
    pub(crate) messages: Option<Vec<Value>>,
    pub(crate) prompt: &'a str,
    pub(crate) config: &'a Value,
    pub(crate) ctx: &'a Context,
    pub(crate) tools: Option<Value>,
    pub(crate) tool_choice: Option<Value>,
}

pub(crate) fn build_body(input: &LlmBodyInput<'_>) -> Result<Value> {
    let mut body = Map::from_iter([("model".to_string(), json!(input.model))]);
    insert_sampling_and_limits(&mut body, input);

    match input.mode {
        LlmMode::Chat => {
            body.insert(
                "messages".to_string(),
                Value::Array(resolve_chat_messages(input)),
            );
            insert_tool_config(&mut body, input);
            if let Some(format) = resolve_response_format(input.config, input.ctx)? {
                body.insert("response_format".to_string(), format);
            }
        }
        LlmMode::Responses => {
            reject_chat_only_parameters(input.config)?;
            body.insert("input".to_string(), Value::String(input.prompt.to_string()));
            if let Some(instructions) = system_prompt(input) {
                body.insert("instructions".to_string(), Value::String(instructions));
            }
        }
    }
    // `extra` is merged last, so a field given there replaces the one built above.
    merge_extra_fields(&mut body, input.config, input.ctx);

    Ok(Value::Object(body))
}

fn insert_sampling_and_limits(body: &mut Map<String, Value>, input: &LlmBodyInput<'_>) {
    if model_supports_temperature(input.model)
        && let Some(temperature) = config_f64(input.config, "temperature", input.ctx)
    {
        body.insert("temperature".to_string(), Value::from(temperature));
    }
    if let Some(max_tokens) = config_u64(input.config, "max_tokens", input.ctx) {
        let key = match input.mode {
            LlmMode::Chat => "max_tokens",
            LlmMode::Responses => "max_output_tokens",
        };
        body.insert(key.to_string(), json!(max_tokens));
    }
    if let Some(max_output_tokens) = config_u64(input.config, "max_output_tokens", input.ctx) {
        let key = match input.mode {
            LlmMode::Chat => "max_completion_tokens",
            LlmMode::Responses => "max_output_tokens",
        };
        body.insert(key.to_string(), json!(max_output_tokens));
    }
}

fn resolve_chat_messages(input: &LlmBodyInput<'_>) -> Vec<Value> {
    if let Some(messages) = &input.messages {
        return messages.clone();
    }

    let mut messages = Vec::new();
    if let Some(system_prompt) = system_prompt(input) {
        messages.push(json!({
            "role": "system",
            "content": system_prompt,
        }));
    }
    messages.push(json!({
        "role": "user",
        "content": input.prompt,
    }));
    messages
}

fn system_prompt(input: &LlmBodyInput<'_>) -> Option<String> {
    input
        .config
        .get("system_prompt")
        .or_else(|| input.config.get("system"))
        .and_then(Value::as_str)
        .map(|prompt| interpolate_ctx(prompt, input.ctx))
}

fn resolve_response_format(config: &Value, ctx: &Context) -> Result<Option<Value>> {
    match config.get("response_format") {
        None | Some(Value::Null) => Ok(None),
        Some(format @ Value::Object(_)) => Ok(Some(interpolate_json_value(format, ctx))),
        Some(_) => anyhow::bail!("llm: 'response_format' must be an object"),
    }
}

/// The Responses API shapes tools and structured output differently, so the
/// chat-shaped parameters fail instead of being dropped from the request.
fn reject_chat_only_parameters(config: &Value) -> Result<()> {
    for key in ["tools", "tool_choice", "response_format"] {
        if config.get(key).is_some_and(|value| !value.is_null()) {
            anyhow::bail!(
                "llm: '{key}' is a chat-mode parameter and is not sent in responses mode; \
                 pass the Responses API field through 'extra' or use mode = \"chat\""
            );
        }
    }
    Ok(())
}

fn insert_tool_config(body: &mut Map<String, Value>, input: &LlmBodyInput<'_>) {
    if let Some(tools) = &input.tools {
        body.insert("tools".to_string(), tools.clone());
    }
    if let Some(tool_choice) = &input.tool_choice {
        body.insert("tool_choice".to_string(), tool_choice.clone());
    }
}

fn merge_extra_fields(body: &mut Map<String, Value>, config: &Value, ctx: &Context) {
    if let Some(extra) = config.get("extra").and_then(Value::as_object) {
        body.extend(
            extra
                .iter()
                .map(|(key, value)| (key.clone(), interpolate_json_value(value, ctx))),
        );
    }
}

fn model_supports_temperature(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    !["o1", "o3", "gpt-5"]
        .iter()
        .any(|prefix| model.starts_with(prefix))
}
