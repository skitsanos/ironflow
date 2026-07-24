use anyhow::Result;
use serde_json::{Map, Value, json};

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;
use crate::util::node_config::{config_f64, config_u64};

use super::config::{LlmMode, interpolate_json_value};

pub(crate) fn resolve_messages(config: &Value, ctx: &Context) -> Result<Option<Vec<Value>>> {
    let Some(messages) = config.get("messages") else {
        return Ok(None);
    };
    let Value::Array(messages) = interpolate_json_value(messages, ctx) else {
        anyhow::bail!("llm: 'messages' must be an array");
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

    anyhow::bail!("llm: either 'prompt', 'input_key', or 'messages' is required")
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
        }
        LlmMode::Responses => {
            body.insert("input".to_string(), Value::String(input.prompt.to_string()));
        }
    }
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
    if let Some(system_prompt) = input
        .config
        .get("system_prompt")
        .or_else(|| input.config.get("system"))
        .and_then(Value::as_str)
    {
        messages.push(json!({
            "role": "system",
            "content": interpolate_ctx(system_prompt, input.ctx),
        }));
    }
    messages.push(json!({
        "role": "user",
        "content": input.prompt,
    }));
    messages
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
