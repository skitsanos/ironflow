use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Map, Value, json};
use std::time::Duration;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::ai_embed_node::resolve_param;

#[derive(Clone, Copy)]
enum LlmMode {
    Chat,
    Responses,
}

impl LlmMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Responses => "responses",
        }
    }
}

#[derive(Clone, Copy)]
enum Provider {
    OpenAI,
    OpenAICompatible,
    Azure,
    Custom,
}

impl Provider {
    fn resolve(config: &serde_json::Value) -> Self {
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

    fn name(self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::OpenAICompatible => "openai_compatible",
            Self::Azure => "azure",
            Self::Custom => "custom",
        }
    }
}

fn interpolate_json_value(value: &serde_json::Value, ctx: &Context) -> serde_json::Value {
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

fn parse_mode(config: &serde_json::Value) -> Result<LlmMode> {
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

fn extract_text(value: &serde_json::Value, out: &mut String) {
    match value {
        Value::String(s) => {
            if !s.is_empty() {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(s);
            }
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

fn extract_chat_tool_calls(data: &serde_json::Value) -> Vec<serde_json::Value> {
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

fn extract_tool_call_names(tool_calls: &[serde_json::Value]) -> Vec<String> {
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

fn extract_chat_reply(data: &serde_json::Value) -> Option<String> {
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

fn extract_responses_reply(data: &serde_json::Value) -> Option<String> {
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

fn parse_timeout(config: &serde_json::Value) -> f64 {
    config
        .get("timeout")
        .and_then(|v| v.as_f64())
        .unwrap_or(30.0)
}

fn resolve_tools(config: &serde_json::Value, ctx: &Context) -> Result<Option<serde_json::Value>> {
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

fn resolve_tool_choice(
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

fn resolve_model(
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

fn resolve_provider_config(
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

fn resolve_messages(config: &serde_json::Value, ctx: &Context) -> Result<Option<Vec<Value>>> {
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

fn resolve_prompt(config: &serde_json::Value, ctx: &Context) -> Result<String> {
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

struct LlmBodyInput<'a> {
    mode: LlmMode,
    model: &'a str,
    messages: Option<Vec<Value>>,
    prompt: &'a str,
    config: &'a serde_json::Value,
    ctx: &'a Context,
    tools: Option<Value>,
    tool_choice: Option<Value>,
}

fn build_body(input: &LlmBodyInput<'_>) -> Result<Value> {
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

    if let Some(temperature) = config.get("temperature").and_then(|v| v.as_f64()) {
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
        body_obj.insert("max_output_tokens".to_string(), json!(max_output_tokens));
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

pub struct LlmNode;

#[async_trait]
impl Node for LlmNode {
    fn node_type(&self) -> &str {
        "llm"
    }

    fn description(&self) -> &str {
        "Run Chat Completions or Responses against OpenAI, OpenAI-compatible, Azure, or custom providers"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let mode = parse_mode(config)?;
        let timeout_s = parse_timeout(config);
        let tools = resolve_tools(config, &ctx)?;
        let tool_choice = resolve_tool_choice(config, &ctx)?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("llm")
            .to_string();

        let messages = resolve_messages(config, &ctx)?;
        let prompt = if messages.is_none() {
            resolve_prompt(config, &ctx)?
        } else {
            String::new()
        };

        let provider = Provider::resolve(config);
        let azure_deployment = if matches!(provider, Provider::Azure) {
            resolve_param(
                config,
                "azure_chat_deployment",
                "AZURE_OPENAI_CHAT_DEPLOYMENT",
                &ctx,
            )
            .or_else(|| {
                resolve_param(
                    config,
                    "azure_responses_deployment",
                    "AZURE_OPENAI_RESPONSES_DEPLOYMENT",
                    &ctx,
                )
            })
            .or_else(|| Some("".to_string()))
        } else {
            None
        };
        let model = resolve_model(config, mode, azure_deployment.as_deref());
        let (url, headers, provider_name) = resolve_provider_config(config, &ctx, mode)?;
        let request_input = LlmBodyInput {
            mode,
            model: &model,
            messages,
            prompt: &prompt,
            config,
            ctx: &ctx,
            tools,
            tool_choice,
        };
        let body = build_body(&request_input)?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs_f64(timeout_s))
            .build()?;

        let response = client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("llm: request failed: {}", e))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("llm: failed to read response body: {}", e))?;

        if !status.is_success() {
            anyhow::bail!(
                "llm: request to {} returned {}: {}",
                provider_name,
                url,
                response_text
            );
        }

        let parsed: Value =
            serde_json::from_str(&response_text).unwrap_or(Value::String(response_text));

        let reply = match mode {
            LlmMode::Chat => extract_chat_reply(&parsed),
            LlmMode::Responses => extract_responses_reply(&parsed),
        };
        let tool_calls = if matches!(mode, LlmMode::Chat) {
            extract_chat_tool_calls(&parsed)
        } else {
            Vec::new()
        };
        let has_tool_calls = !tool_calls.is_empty();
        let tool_call_names = extract_tool_call_names(&tool_calls);

        let mut output = NodeOutput::new();
        output.insert(format!("{}_model", output_key), Value::String(model));
        output.insert(
            format!("{}_provider", output_key),
            Value::String(provider.name().to_string()),
        );
        output.insert(
            format!("{}_mode", output_key),
            Value::String(mode.as_str().to_string()),
        );
        output.insert(
            format!("{}_status", output_key),
            Value::Number(status.as_u16().into()),
        );
        output.insert(format!("{}_raw", output_key), parsed);
        output.insert(format!("{}_success", output_key), Value::Bool(true));
        if let Some(usage) = output
            .get(&format!("{}_raw", output_key))
            .and_then(|value| value.get("usage"))
        {
            output.insert(format!("{}_usage", output_key), usage.clone());
        }
        if let Some(reply) = reply {
            output.insert(format!("{}_text", output_key), Value::String(reply));
        } else if !tool_calls.is_empty() {
            output.insert(
                format!("{}_text", output_key),
                Value::String("<tool call requested>".to_string()),
            );
        } else {
            output.insert(
                format!("{}_text", output_key),
                Value::String("<no reply>".to_string()),
            );
        }
        output.insert(
            format!("{}_tool_calls", output_key),
            Value::Array(tool_calls),
        );
        output.insert(
            format!("{}_tool_call_needed", output_key),
            Value::Bool(has_tool_calls),
        );
        output.insert(
            format!("{}_tool_call_names", output_key),
            Value::Array(
                tool_call_names
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );

        Ok(output)
    }
}
