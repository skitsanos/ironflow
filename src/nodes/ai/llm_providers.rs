use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value};

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

use super::embeddings::resolve_param;

#[derive(Clone, Copy)]
pub(super) enum LlmMode {
    Chat,
    Responses,
}

impl LlmMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Responses => "responses",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Provider {
    OpenAI,
    OpenAICompatible,
    Azure,
    Custom,
}

impl Provider {
    pub(super) fn resolve(config: &serde_json::Value) -> Self {
        match config
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("openai")
            .to_ascii_lowercase()
            .as_str()
        {
            "openai_compatible" | "compatible" => Self::OpenAICompatible,
            "azure" => Self::Azure,
            "custom" => Self::Custom,
            _ => Self::OpenAI,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::OpenAICompatible => "openai_compatible",
            Self::Azure => "azure",
            Self::Custom => "custom",
        }
    }
}

pub(super) fn interpolate_json_value(
    value: &serde_json::Value,
    ctx: &Context,
) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(interpolate_ctx(s, ctx)),
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| interpolate_json_value(v, ctx)).collect())
        }
        serde_json::Value::Object(map) => {
            let out: Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), interpolate_json_value(v, ctx)))
                .collect();
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    }
}

pub(super) fn parse_mode(config: &serde_json::Value) -> Result<LlmMode> {
    let mode = config
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("chat")
        .to_ascii_lowercase();

    match mode.as_str() {
        "chat" => Ok(LlmMode::Chat),
        "responses" => Ok(LlmMode::Responses),
        "auto" => {
            if config.get("messages").is_some() {
                Ok(LlmMode::Chat)
            } else if config
                .get("responses_input")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                Ok(LlmMode::Responses)
            } else {
                Ok(LlmMode::Chat)
            }
        }
        _ => anyhow::bail!(
            "llm: unsupported mode '{}'. Use 'chat', 'responses', or 'auto'.",
            mode
        ),
    }
}

pub(super) fn parse_timeout(config: &serde_json::Value) -> f64 {
    config
        .get("timeout")
        .and_then(|v| v.as_f64())
        .unwrap_or(30.0)
}

pub(super) fn optional_u64_config(config: &serde_json::Value, key: &str) -> Option<u64> {
    config.get(key).and_then(|value| match value {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => s.parse::<u64>().ok(),
        _ => None,
    })
}

pub(super) fn resolve_tools(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<Option<serde_json::Value>> {
    let Some(raw_tools) = config.get("tools") else {
        return Ok(None);
    };

    let tools = interpolate_json_value(raw_tools, ctx);
    match tools {
        serde_json::Value::Array(_) => Ok(Some(tools)),
        serde_json::Value::Object(_) => Ok(Some(serde_json::Value::Array(vec![tools]))),
        _ => anyhow::bail!("llm: 'tools' must be an array of tool objects"),
    }
}

pub(super) fn resolve_tool_choice(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<Option<serde_json::Value>> {
    let Some(raw_tool_choice) = config.get("tool_choice") else {
        return Ok(None);
    };

    let tool_choice = interpolate_json_value(raw_tool_choice, ctx);
    match tool_choice {
        serde_json::Value::String(_) | serde_json::Value::Object(_) => Ok(Some(tool_choice)),
        _ => anyhow::bail!("llm: 'tool_choice' must be a string or object"),
    }
}

pub(super) fn resolve_model(
    config: &serde_json::Value,
    mode: LlmMode,
    azure_deployment: Option<&str>,
) -> String {
    let key = if matches!(mode, LlmMode::Chat) {
        "chat_model"
    } else {
        "responses_model"
    };

    if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
        return model.to_string();
    }
    if let Some(model) = config.get(key).and_then(|v| v.as_str()) {
        return model.to_string();
    }

    if let Some(deployment) = azure_deployment {
        return deployment.to_string();
    }

    "gpt-5-mini".to_string()
}

fn model_supports_temperature(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    !["o1", "o3", "gpt-5"]
        .iter()
        .any(|prefix| model.starts_with(prefix))
}

pub(super) fn resolve_provider_config(
    config: &serde_json::Value,
    ctx: &Context,
    mode: LlmMode,
) -> Result<(String, HeaderMap, String)> {
    let provider = Provider::resolve(config);
    let mut headers = HeaderMap::new();

    match provider {
        Provider::OpenAI | Provider::OpenAICompatible => {
            let base_url = if matches!(provider, Provider::OpenAICompatible) {
                resolve_param(
                    config,
                    "base_url",
                    "OPENAI_COMPATIBLE_BASE_URL",
                    ctx,
                )
                .or_else(|| resolve_param(config, "base_url", "LLM_BASE_URL", ctx))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                    "llm ({}) requires 'base_url' or OPENAI_COMPATIBLE_BASE_URL/LLM_BASE_URL",
                        provider.name()
                    )
                })?
            } else {
                resolve_param(config, "base_url", "OPENAI_BASE_URL", ctx)
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
            };

            let api_key =
                resolve_param(config, "api_key", "OPENAI_API_KEY", ctx).ok_or_else(|| {
                    anyhow::anyhow!("llm (openai) requires 'api_key' or OPENAI_API_KEY")
                })?;
            headers.insert(
                HeaderName::from_static("authorization"),
                HeaderValue::from_str(&format!("Bearer {api_key}"))?,
            );

            let endpoint = if matches!(mode, LlmMode::Chat) {
                format!("{}/chat/completions", base_url.trim_end_matches('/'))
            } else {
                format!("{}/responses", base_url.trim_end_matches('/'))
            };
            Ok((endpoint, headers, provider.name().to_string()))
        }
        Provider::Azure => {
            let endpoint = resolve_param(config, "azure_endpoint", "AZURE_OPENAI_ENDPOINT", ctx)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "llm (azure) requires 'azure_endpoint' or AZURE_OPENAI_ENDPOINT"
                    )
                })?;
            let api_version =
                resolve_param(config, "azure_api_version", "AZURE_OPENAI_API_VERSION", ctx)
                    .unwrap_or_else(|| "2024-08-01-preview".to_string());

            let chat_deployment = resolve_param(
                config,
                "azure_chat_deployment",
                "AZURE_OPENAI_CHAT_DEPLOYMENT",
                ctx,
            );
            let responses_deployment = resolve_param(
                config,
                "azure_responses_deployment",
                "AZURE_OPENAI_RESPONSES_DEPLOYMENT",
                ctx,
            );
            let deployment = if matches!(mode, LlmMode::Chat) {
                chat_deployment.or_else(|| responses_deployment.clone())
            } else {
                responses_deployment.or_else(|| chat_deployment.clone())
            };
            let deployment = deployment.ok_or_else(|| {
                anyhow::anyhow!("llm (azure) requires deployment for selected mode")
            })?;

            let api_key = resolve_param(config, "api_key", "AZURE_OPENAI_API_KEY", ctx)
                .ok_or_else(|| {
                    anyhow::anyhow!("llm (azure) requires 'api_key' or AZURE_OPENAI_API_KEY")
                })?;
            headers.insert(
                HeaderName::from_static("api-key"),
                HeaderValue::from_str(&api_key)?,
            );

            let path = if matches!(mode, LlmMode::Chat) {
                "chat/completions"
            } else {
                "responses"
            };
            let endpoint = format!(
                "{}/openai/deployments/{}/{}?api-version={}",
                endpoint.trim_end_matches('/'),
                deployment,
                path,
                api_version
            );
            Ok((endpoint, headers, provider.name().to_string()))
        }
        Provider::Custom => {
            let base_url = resolve_param(config, "base_url", "LLM_BASE_URL", ctx).ok_or_else(|| {
                anyhow::anyhow!(
                    "llm (custom) requires 'base_url' or LLM_BASE_URL when using custom provider"
                )
            })?;
            let path = if matches!(mode, LlmMode::Chat) {
                config
                    .get("chat_path")
                    .or_else(|| config.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("/chat/completions")
            } else {
                config
                    .get("responses_path")
                    .or_else(|| config.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("/responses")
            };

            let endpoint = if path.starts_with("http://") || path.starts_with("https://") {
                path.to_string()
            } else {
                format!(
                    "{}{}{}",
                    base_url.trim_end_matches('/'),
                    if path.starts_with('/') { "" } else { "/" },
                    path
                )
            };

            let auth_type = config
                .get("auth_type")
                .and_then(|v| v.as_str())
                .unwrap_or("bearer")
                .to_ascii_lowercase();
            match auth_type.as_str() {
                "none" => {}
                "bearer" => {
                    if let Some(token) = config.get("api_key").and_then(|v| v.as_str()) {
                        headers.insert(
                            HeaderName::from_static("authorization"),
                            HeaderValue::from_str(&format!(
                                "Bearer {}",
                                interpolate_ctx(token, ctx)
                            ))?,
                        );
                    }
                }
                "api_key" => {
                    if let Some(api_key) = config.get("api_key").and_then(|v| v.as_str()) {
                        let header_name = config
                            .get("auth_header")
                            .and_then(|v| v.as_str())
                            .unwrap_or("x-api-key")
                            .to_lowercase();
                        headers.insert(
                            HeaderName::from_bytes(header_name.as_bytes())?,
                            HeaderValue::from_str(&interpolate_ctx(api_key, ctx))?,
                        );
                    }
                }
                other => {
                    anyhow::bail!(
                        "llm: unsupported auth_type '{}' for custom provider; use 'bearer', 'api_key', or 'none'",
                        other
                    )
                }
            }

            Ok((endpoint, headers, provider.name().to_string()))
        }
    }
}

pub(super) fn resolve_messages(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<Option<Vec<Value>>> {
    let Some(messages_value) = config.get("messages") else {
        return Ok(None);
    };

    let interpolated = interpolate_json_value(messages_value, ctx);
    let Value::Array(items) = interpolated else {
        anyhow::bail!("llm: 'messages' must be an array");
    };

    if items.is_empty() {
        return Ok(None);
    }

    Ok(Some(items))
}

pub(super) fn resolve_prompt(config: &serde_json::Value, ctx: &Context) -> Result<String> {
    if let Some(prompt) = config.get("prompt").and_then(|v| v.as_str()) {
        return Ok(interpolate_ctx(prompt, ctx));
    }

    if let Some(input_key) = config.get("input_key").and_then(|v| v.as_str()) {
        let key = interpolate_ctx(input_key, ctx);
        return ctx
            .get(&key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("llm: input_key '{}' not found or not a string", key));
    }

    anyhow::bail!("llm: either 'prompt', 'input_key', or 'messages' is required");
}

pub(super) struct LlmBodyInput<'a> {
    pub(super) mode: LlmMode,
    pub(super) model: &'a str,
    pub(super) messages: Option<Vec<Value>>,
    pub(super) prompt: &'a str,
    pub(super) config: &'a serde_json::Value,
    pub(super) ctx: &'a Context,
    pub(super) tools: Option<Value>,
    pub(super) tool_choice: Option<Value>,
}

pub(super) fn build_body(input: &LlmBodyInput<'_>) -> Result<Value> {
    use serde_json::json;

    let LlmBodyInput {
        mode,
        model,
        messages,
        prompt,
        config,
        ctx,
        tools,
        tool_choice,
    } = input;
    let body = json!({ "model": model });

    let mut body_obj = body
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("llm: failed to initialize request body"))?;

    if model_supports_temperature(model)
        && let Some(temperature) = config.get("temperature").and_then(|v| v.as_f64())
    {
        body_obj.insert("temperature".to_string(), Value::from(temperature));
    }
    if let Some(max_tokens) = config.get("max_tokens").and_then(|v| v.as_u64()) {
        if matches!(mode, LlmMode::Responses) {
            body_obj.insert("max_output_tokens".to_string(), json!(max_tokens));
        } else {
            body_obj.insert("max_tokens".to_string(), json!(max_tokens));
        }
    }
    if let Some(max_output_tokens) = config.get("max_output_tokens").and_then(|v| v.as_u64()) {
        if matches!(mode, LlmMode::Chat) {
            body_obj.insert(
                "max_completion_tokens".to_string(),
                json!(max_output_tokens),
            );
        } else {
            body_obj.insert("max_output_tokens".to_string(), json!(max_output_tokens));
        }
    }

    if matches!(mode, LlmMode::Chat) {
        let final_messages = if let Some(messages) = messages.as_ref() {
            messages.clone()
        } else {
            let mut items = Vec::new();
            if let Some(system_prompt) = config
                .get("system_prompt")
                .or_else(|| config.get("system"))
                .and_then(|v| v.as_str())
            {
                items.push(json!({
                    "role": "system",
                    "content": interpolate_ctx(system_prompt, ctx),
                }));
            }

            items.push(json!({
                "role": "user",
                "content": prompt,
            }));
            items
        };
        body_obj.insert("messages".to_string(), Value::Array(final_messages));
    } else {
        body_obj.insert("input".to_string(), Value::String(prompt.to_string()));
    }

    if matches!(mode, LlmMode::Chat)
        && let Some(tools) = tools
    {
        body_obj.insert("tools".to_string(), tools.clone());
    }
    if matches!(mode, LlmMode::Chat)
        && let Some(tool_choice) = tool_choice
    {
        body_obj.insert("tool_choice".to_string(), tool_choice.clone());
    }

    if let Some(extra) = config.get("extra").and_then(|v| v.as_object()) {
        for (k, v) in extra {
            body_obj.insert(k.clone(), interpolate_json_value(v, ctx));
        }
    }

    Ok(Value::Object(body_obj))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_reasoning_model_omits_temperature_and_maps_max_output_tokens() {
        let config = serde_json::json!({
            "temperature": 0.2,
            "max_output_tokens": 123,
        });
        let ctx = Context::new();
        let body = build_body(&LlmBodyInput {
            mode: LlmMode::Chat,
            model: "gpt-5",
            messages: None,
            prompt: "hello",
            config: &config,
            ctx: &ctx,
            tools: None,
            tool_choice: None,
        })
        .unwrap();

        assert!(body.get("temperature").is_none());
        assert_eq!(body.get("max_completion_tokens"), Some(&json!(123)));
        assert!(body.get("max_output_tokens").is_none());
    }

    #[test]
    fn chat_non_reasoning_model_keeps_temperature() {
        let config = serde_json::json!({
            "temperature": 0.2,
            "max_output_tokens": 123,
        });
        let ctx = Context::new();
        let body = build_body(&LlmBodyInput {
            mode: LlmMode::Chat,
            model: "gpt-4o-mini",
            messages: None,
            prompt: "hello",
            config: &config,
            ctx: &ctx,
            tools: None,
            tool_choice: None,
        })
        .unwrap();

        assert_eq!(body.get("temperature"), Some(&json!(0.2)));
        assert_eq!(body.get("max_completion_tokens"), Some(&json!(123)));
    }

    #[test]
    fn responses_mode_uses_max_output_tokens() {
        let config = serde_json::json!({
            "temperature": 0.2,
            "max_output_tokens": 123,
        });
        let ctx = Context::new();
        let body = build_body(&LlmBodyInput {
            mode: LlmMode::Responses,
            model: "gpt-5",
            messages: None,
            prompt: "hello",
            config: &config,
            ctx: &ctx,
            tools: None,
            tool_choice: None,
        })
        .unwrap();

        assert!(body.get("temperature").is_none());
        assert_eq!(body.get("max_output_tokens"), Some(&json!(123)));
        assert!(body.get("max_completion_tokens").is_none());
    }
}
