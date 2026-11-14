use anyhow::Result;
use async_trait::async_trait;
use futures_util::TryStreamExt;
use serde_json::Value;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::duration::positive_duration;
use crate::util::limits;
use crate::util::redaction::SecretRedactor;
use crate::util::sensitive_url::{SecretEndpoint, redact_sensitive_text};

use super::embeddings::resolve_param;
use super::llm_providers::{
    LlmBodyInput, LlmMode, Provider, build_body, optional_u64_config, parse_mode, parse_timeout,
    resolve_messages, resolve_model, resolve_prompt, resolve_provider_config, resolve_tool_choice,
    resolve_tools,
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
        .map_err(|e| anyhow::anyhow!("llm: failed to read response body: {}", e.without_url()))?
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

    #[allow(clippy::too_many_lines)] // One provider request lifecycle: config, request, response and artifacts.
    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        // The provider decides where the prompt and the credential go, so an
        // unknown value fails before anything else is resolved or sent.
        let provider = Provider::resolve(config)?;
        let mode = parse_mode(config, ctx)?;
        let timeout_s = parse_timeout(config, ctx)?;
        let tools = resolve_tools(config, ctx)?;
        let tool_choice = resolve_tool_choice(config, ctx)?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("llm")
            .to_string();
        let max_response_bytes = optional_u64_config(config, "max_response_bytes", ctx)
            .filter(|limit| *limit > 0)
            .or_else(limits::max_llm_response_bytes);

        let messages = resolve_messages(config, ctx)?;
        if matches!(mode, LlmMode::Responses) && messages.is_some() {
            anyhow::bail!("llm: 'messages' and 'messages_key' are supported only in chat mode");
        }
        let (messages, artifact_inputs) = match messages {
            Some(messages) => {
                let (messages, artifact_inputs) =
                    super::llm_message_artifacts::resolve(messages, config, ctx).await?;
                (Some(messages), artifact_inputs)
            }
            None => (None, false),
        };
        let prompt = if messages.is_none() {
            resolve_prompt(config, ctx)?
        } else {
            String::new()
        };

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
        let (url, mut headers, provider_name) =
            resolve_provider_config(config, ctx, mode, provider)?;
        // These headers contain resolved auth, including keys supplied only by the environment.
        let credentials = headers
            .values_mut()
            .map(|value| {
                value.set_sensitive(true);
                String::from_utf8_lossy(value.as_bytes()).into_owned()
            })
            .collect::<Vec<_>>();
        // Resolved credentials are redacted at any length (IF-108); the
        // length floor of overlay values (IF-176) does not apply to them.
        let redactor = SecretRedactor::from_credentials(credentials.iter().map(String::as_str));
        let redact_error = |text: &str| redact_sensitive_text(&redactor.redact_text(text));
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

        let client = crate::util::provider_http::client_builder()
            .timeout(positive_duration(timeout_s, "llm timeout")?)
            .build()?;

        let response = client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "{}",
                    redact_error(&format!(
                        "llm: request failed for {}: {}",
                        SecretEndpoint::new(&url),
                        error.without_url()
                    ))
                )
            })?;

        let status = response.status();
        let response_text = read_capped_response_body(response, max_response_bytes)
            .await
            .map_err(|error| anyhow::anyhow!("{}", redact_error(&error.to_string())))?;
        let response_text = if artifact_inputs {
            super::llm_message_artifacts::redact_data_urls(&response_text)
        } else {
            response_text
        };

        if !status.is_success() {
            anyhow::bail!(
                "{}",
                redact_error(&format!(
                    "llm: request to {} at {} returned {}: {}",
                    provider_name,
                    SecretEndpoint::new(&url),
                    status,
                    response_text
                ))
            );
        }

        let parsed: Value =
            serde_json::from_str(&response_text).unwrap_or(Value::String(response_text));

        super::llm_output::build(
            parsed,
            super::llm_output::ReplyMeta {
                output_key: &output_key,
                model,
                provider: provider.name(),
                mode,
                status: status.as_u16(),
            },
        )
    }
}
