use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

/// Simple percent-encoding for form data values.
pub(crate) fn percent_encode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

/// Resolve a config string parameter, falling back to an environment variable.
pub(crate) fn resolve_param(
    config: &serde_json::Value,
    key: &str,
    env_key: &str,
    ctx: &Context,
) -> Option<String> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| interpolate_ctx(s, ctx))
        .or_else(|| std::env::var(env_key).ok())
}

// -- OAuth token cache --

struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

static OAUTH_TOKEN_CACHE: Mutex<Option<CachedToken>> = Mutex::new(None);

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

// -- OpenAI-compatible response types --

#[derive(Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f64>,
}

#[derive(Deserialize)]
struct OpenAiErrorResponse {
    error: OpenAiErrorDetail,
}

#[derive(Deserialize)]
struct OpenAiErrorDetail {
    message: String,
}

// -- Ollama response types --

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f64>>,
}

#[derive(Deserialize)]
struct OllamaErrorResponse {
    error: String,
}

// -- Provider helpers --

pub(crate) async fn embed_openai(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    texts: &[String],
) -> Result<Vec<Vec<f64>>> {
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let body = serde_json::json!({ "model": model, "input": texts });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("OpenAI embedding request failed: {}", e))?;

    let status = response.status();
    let resp_body = response
        .text()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read OpenAI response: {}", e))?;

    if !status.is_success() {
        if let Ok(err) = serde_json::from_str::<OpenAiErrorResponse>(&resp_body) {
            anyhow::bail!("OpenAI API error ({}): {}", status, err.error.message);
        }
        anyhow::bail!("OpenAI API error ({}): {}", status, resp_body);
    }

    let parsed: OpenAiEmbeddingResponse = serde_json::from_str(&resp_body)
        .map_err(|e| anyhow::anyhow!("Failed to parse OpenAI response: {}", e))?;

    Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
}

pub(crate) async fn embed_ollama(
    client: &reqwest::Client,
    host: &str,
    model: &str,
    texts: &[String],
) -> Result<Vec<Vec<f64>>> {
    let url = format!("{}/api/embed", host.trim_end_matches('/'));
    let body = serde_json::json!({ "model": model, "input": texts });

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Ollama embedding request failed: {}", e))?;

    let status = response.status();
    let resp_body = response
        .text()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read Ollama response: {}", e))?;

    if !status.is_success() {
        if let Ok(err) = serde_json::from_str::<OllamaErrorResponse>(&resp_body) {
            anyhow::bail!("Ollama error ({}): {}", status, err.error);
        }
        anyhow::bail!("Ollama error ({}): {}", status, resp_body);
    }

    let parsed: OllamaEmbedResponse = serde_json::from_str(&resp_body)
        .map_err(|e| anyhow::anyhow!("Failed to parse Ollama response: {}", e))?;

    Ok(parsed.embeddings)
}

pub(crate) async fn acquire_oauth_token(
    client: &reqwest::Client,
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    scope: Option<&str>,
) -> Result<String> {
    // Check cache
    {
        let cache = OAUTH_TOKEN_CACHE
            .lock()
            .map_err(|e| anyhow::anyhow!("OAuth token cache lock poisoned: {e}"))?;
        if let Some(ref cached) = *cache
            && Instant::now() + Duration::from_secs(60) < cached.expires_at
        {
            return Ok(cached.access_token.clone());
        }
    }

    // Fetch new token via form-encoded POST
    let mut form_body = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}",
        percent_encode(client_id),
        percent_encode(client_secret),
    );
    if let Some(s) = scope {
        form_body.push_str(&format!("&scope={}", percent_encode(s)));
    }

    let response = client
        .post(token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("OAuth token request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read OAuth token response: {}", e))?;

    if !status.is_success() {
        anyhow::bail!("OAuth token request failed ({}): {}", status, body);
    }

    let token_resp: TokenResponse = serde_json::from_str(&body)
        .map_err(|e| anyhow::anyhow!("Failed to parse OAuth token response: {}", e))?;

    let expires_at = Instant::now() + Duration::from_secs(token_resp.expires_in);
    let access_token = token_resp.access_token.clone();

    // Update cache
    let mut cache = OAUTH_TOKEN_CACHE
        .lock()
        .map_err(|e| anyhow::anyhow!("OAuth token cache lock poisoned: {e}"))?;
    *cache = Some(CachedToken {
        access_token: token_resp.access_token,
        expires_at,
    });

    Ok(access_token)
}

// -- Node --

pub struct AiEmbedNode;

#[async_trait]
impl Node for AiEmbedNode {
    fn node_type(&self) -> &str {
        "ai_embed"
    }

    fn description(&self) -> &str {
        "Generate text embeddings via OpenAI, Ollama, or OAuth providers"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let provider = config
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("openai");

        let input_key = config
            .get("input_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("ai_embed requires 'input_key' parameter"))?;
        let input_key = crate::lua::interpolate::interpolate_ctx(input_key, &ctx);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("embed");

        let timeout_s = config
            .get("timeout")
            .and_then(|v| v.as_f64())
            .unwrap_or(120.0);

        // Get input texts from context
        let input_value = ctx.get(&input_key).ok_or_else(|| {
            anyhow::anyhow!("ai_embed: input_key '{}' not found in context", input_key)
        })?;

        let texts: Vec<String> = match input_value {
            serde_json::Value::String(s) => vec![s.clone()],
            serde_json::Value::Array(arr) => arr
                .iter()
                .map(|v| match v {
                    serde_json::Value::String(s) => Ok(s.clone()),
                    other => Ok(other.to_string()),
                })
                .collect::<Result<Vec<String>>>()?,
            other => vec![other.to_string()],
        };

        if texts.is_empty() {
            let mut output = NodeOutput::new();
            output.insert(format!("{}_embeddings", output_key), serde_json::json!([]));
            output.insert(format!("{}_count", output_key), serde_json::json!(0));
            output.insert(format!("{}_dimension", output_key), serde_json::json!(0));
            output.insert(format!("{}_model", output_key), serde_json::json!(""));
            output.insert(
                format!("{}_success", output_key),
                serde_json::Value::Bool(true),
            );
            return Ok(output);
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs_f64(timeout_s))
            .build()?;

        let (embeddings, model_used) = match provider {
            "openai" => {
                let api_key =
                    resolve_param(config, "api_key", "OPENAI_API_KEY", &ctx).ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_embed (openai) requires 'api_key' or OPENAI_API_KEY env var"
                        )
                    })?;
                let base_url = resolve_param(config, "base_url", "OPENAI_BASE_URL", &ctx)
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
                let model = config
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text-embedding-3-small")
                    .to_string();

                let embs = embed_openai(&client, &base_url, &api_key, &model, &texts).await?;
                (embs, model)
            }
            "ollama" => {
                let host = resolve_param(config, "ollama_host", "OLLAMA_HOST", &ctx)
                    .unwrap_or_else(|| "http://localhost:11434".to_string());
                let model = config
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("nomic-embed-text")
                    .to_string();

                let embs = embed_ollama(&client, &host, &model, &texts).await?;
                (embs, model)
            }
            "oauth" => {
                let token_url = resolve_param(config, "token_url", "OAUTH_TOKEN_URL", &ctx)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_embed (oauth) requires 'token_url' or OAUTH_TOKEN_URL env var"
                        )
                    })?;
                let client_id = resolve_param(config, "client_id", "OAUTH_CLIENT_ID", &ctx)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_embed (oauth) requires 'client_id' or OAUTH_CLIENT_ID env var"
                        )
                    })?;
                let client_secret = resolve_param(
                    config,
                    "client_secret",
                    "OAUTH_CLIENT_SECRET",
                    &ctx,
                )
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "ai_embed (oauth) requires 'client_secret' or OAUTH_CLIENT_SECRET env var"
                    )
                })?;
                let scope = resolve_param(config, "scope", "OAUTH_SCOPE", &ctx);
                let base_url = resolve_param(config, "base_url", "OAUTH_BASE_URL", &ctx)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_embed (oauth) requires 'base_url' or OAUTH_BASE_URL env var"
                        )
                    })?;
                let model = config
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("openai-text-embedding-3-small")
                    .to_string();

                let token = acquire_oauth_token(
                    &client,
                    &token_url,
                    &client_id,
                    &client_secret,
                    scope.as_deref(),
                )
                .await?;

                let embs = embed_openai(&client, &base_url, &token, &model, &texts).await?;
                (embs, model)
            }
            other => anyhow::bail!("ai_embed: unsupported provider '{}'", other),
        };

        let count = embeddings.len();
        let dimension = embeddings.first().map(|v| v.len()).unwrap_or(0);

        let embeddings_json: Vec<serde_json::Value> = embeddings
            .into_iter()
            .map(|vec| serde_json::json!(vec))
            .collect();

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_embeddings", output_key),
            serde_json::Value::Array(embeddings_json),
        );
        output.insert(format!("{}_count", output_key), serde_json::json!(count));
        output.insert(
            format!("{}_dimension", output_key),
            serde_json::json!(dimension),
        );
        output.insert(
            format!("{}_model", output_key),
            serde_json::json!(model_used),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );

        Ok(output)
    }
}
