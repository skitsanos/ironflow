use anyhow::Result;
use async_trait::async_trait;
use futures_util::TryStreamExt;
use serde_json::Value;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::limits;

use super::embeddings::resolve_param;
use super::llm_providers::{
    LlmBodyInput, LlmMode, Provider, build_body, optional_u64_config, parse_mode, parse_timeout,
    resolve_messages, resolve_model, resolve_prompt, resolve_provider_config, resolve_tool_choice,
    resolve_tools,
};
use super::llm_response::{
    extract_chat_reply, extract_chat_tool_calls, extract_responses_reply, extract_tool_call_names,
    normalize_tool_calls,
};

async fn read_capped_response_body(
    response: reqwest::Response,
    max_bytes: Option<u64>,
) -> Result<String> {
    if let Some(max_bytes) = max_bytes
        && let Some(content_length) = response.content_length()
        && content_length > max_bytes
    {
        anyhow::bail!(
            "llm: response body content-length {} exceeds max_response_bytes limit of {}",
            content_length,
            max_bytes
        );
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream
        .try_next()
        .await
        .map_err(|e| anyhow::anyhow!("llm: failed to read response body: {}", e))?
    {
        if let Some(max_bytes) = max_bytes
            && body.len() as u64 + chunk.len() as u64 > max_bytes
        {
            anyhow::bail!(
                "llm: response body exceeded max_response_bytes limit of {}",
                max_bytes
            );
        }
        body.extend_from_slice(&chunk);
    }

    String::from_utf8(body)
        .map_err(|e| anyhow::anyhow!("llm: response body is not valid UTF-8: {}", e))
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

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let mode = parse_mode(config)?;
        let timeout_s = parse_timeout(config);
        let tools = resolve_tools(config, ctx)?;
        let tool_choice = resolve_tool_choice(config, ctx)?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("llm")
            .to_string();
        let max_response_bytes = optional_u64_config(config, "max_response_bytes")
            .filter(|limit| *limit > 0)
            .or_else(limits::max_llm_response_bytes);

        let messages = resolve_messages(config, ctx)?;
        let prompt = if messages.is_none() {
            resolve_prompt(config, ctx)?
        } else {
            String::new()
        };

        let provider = Provider::resolve(config);
        let azure_deployment = if matches!(provider, Provider::Azure) {
            resolve_param(
                config,
                "azure_chat_deployment",
                "AZURE_OPENAI_CHAT_DEPLOYMENT",
                ctx,
            )
            .or_else(|| {
                resolve_param(
                    config,
                    "azure_responses_deployment",
                    "AZURE_OPENAI_RESPONSES_DEPLOYMENT",
                    ctx,
                )
            })
            .or_else(|| Some("".to_string()))
        } else {
            None
        };
        let model = resolve_model(config, mode, azure_deployment.as_deref());
        let (url, headers, provider_name) = resolve_provider_config(config, ctx, mode)?;
        let request_input = LlmBodyInput {
            mode,
            model: &model,
            messages,
            prompt: &prompt,
            config,
            ctx,
            tools,
            tool_choice,
        };
        let body = build_body(&request_input)?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs_f64(timeout_s))
            .build()?;

        let response = client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("llm: request failed: {}", e))?;

        let status = response.status();
        let response_text = read_capped_response_body(response, max_response_bytes).await?;

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
        let normalized_tool_calls = normalize_tool_calls(&tool_calls);

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
        output.insert(
            format!("{}_tool_calls_normalized", output_key),
            Value::Array(normalized_tool_calls),
        );

        Ok(output)
    }
}
