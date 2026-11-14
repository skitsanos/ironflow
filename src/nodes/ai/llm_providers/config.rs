use anyhow::Result;
use serde_json::Value;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_value;
use crate::util::node_config::{config_bool, config_f64_or, config_u64};

#[derive(Clone, Copy)]
pub(crate) enum LlmMode {
    Chat,
    Responses,
}

impl LlmMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Responses => "responses",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Provider {
    OpenAI,
    OpenAICompatible,
    Azure,
    Custom,
}

const ACCEPTED_PROVIDERS: &str = "Use 'openai', 'openai_compatible', 'azure', or 'custom'.";

impl Provider {
    /// Selects the backend that receives the prompt and the credential.
    ///
    /// An unrecognised value is an error, never a fallback: a misspelt
    /// provider must not route a request to the OpenAI configuration.
    pub(crate) fn resolve(config: &Value) -> Result<Self> {
        let name = match config.get("provider") {
            None | Some(Value::Null) => return Ok(Self::OpenAI),
            Some(Value::String(name)) => name,
            Some(_) => anyhow::bail!("llm: 'provider' must be a string. {ACCEPTED_PROVIDERS}"),
        };
        match name.to_ascii_lowercase().as_str() {
            "openai" => Ok(Self::OpenAI),
            "openai_compatible" | "compatible" => Ok(Self::OpenAICompatible),
            "azure" => Ok(Self::Azure),
            "custom" => Ok(Self::Custom),
            _ => anyhow::bail!(
                "llm: unsupported provider '{}'. {ACCEPTED_PROVIDERS}",
                name.chars().take(64).collect::<String>()
            ),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::OpenAICompatible => "openai_compatible",
            Self::Azure => "azure",
            Self::Custom => "custom",
        }
    }
}

pub(crate) fn interpolate_json_value(value: &Value, ctx: &Context) -> Value {
    interpolate_value(value, ctx)
}

pub(crate) fn parse_mode(config: &Value, ctx: &Context) -> Result<LlmMode> {
    let mode = config
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("chat")
        .to_ascii_lowercase();

    match mode.as_str() {
        "chat" => Ok(LlmMode::Chat),
        "responses" => Ok(LlmMode::Responses),
        "auto" if config.get("messages").is_some() => Ok(LlmMode::Chat),
        "auto" if config_bool(config, "responses_input", ctx).unwrap_or(false) => {
            Ok(LlmMode::Responses)
        }
        "auto" => Ok(LlmMode::Chat),
        _ => anyhow::bail!(
            "llm: unsupported mode '{}'. Use 'chat', 'responses', or 'auto'.",
            mode
        ),
    }
}

pub(crate) fn parse_timeout(config: &Value, ctx: &Context) -> Result<f64> {
    config_f64_or(config, "timeout", ctx, 30.0)
}

pub(crate) fn optional_u64_config(config: &Value, key: &str, ctx: &Context) -> Option<u64> {
    config_u64(config, key, ctx)
}

pub(crate) fn resolve_tools(config: &Value, ctx: &Context) -> Result<Option<Value>> {
    let Some(raw_tools) = config.get("tools") else {
        return Ok(None);
    };

    let tools = interpolate_json_value(raw_tools, ctx);
    match tools {
        Value::Array(_) => Ok(Some(tools)),
        Value::Object(_) => Ok(Some(Value::Array(vec![tools]))),
        _ => anyhow::bail!("llm: 'tools' must be an array of tool objects"),
    }
}

pub(crate) fn resolve_tool_choice(config: &Value, ctx: &Context) -> Result<Option<Value>> {
    let Some(raw_tool_choice) = config.get("tool_choice") else {
        return Ok(None);
    };

    let tool_choice = interpolate_json_value(raw_tool_choice, ctx);
    match tool_choice {
        Value::String(_) | Value::Object(_) => Ok(Some(tool_choice)),
        _ => anyhow::bail!("llm: 'tool_choice' must be a string or object"),
    }
}

pub(crate) fn resolve_model(
    config: &Value,
    mode: LlmMode,
    azure_deployment: Option<&str>,
) -> String {
    let mode_key = match mode {
        LlmMode::Chat => "chat_model",
        LlmMode::Responses => "responses_model",
    };

    config
        .get("model")
        .and_then(Value::as_str)
        .or_else(|| config.get(mode_key).and_then(Value::as_str))
        .or(azure_deployment)
        .unwrap_or("gpt-5-mini")
        .to_string()
}
