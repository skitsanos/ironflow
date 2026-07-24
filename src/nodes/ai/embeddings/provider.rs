use anyhow::Result;

use crate::engine::types::Context;

use super::config::resolve_param;
use super::oauth::acquire_oauth_token;
use super::response::{parse_ollama_response, parse_openai_response};

pub(in crate::nodes::ai) async fn embed_openai(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    texts: &[String],
) -> Result<Vec<Vec<f64>>> {
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({ "model": model, "input": texts }))
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("OpenAI embedding request failed: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| anyhow::anyhow!("Failed to read OpenAI response: {}", error))?;

    parse_openai_response(status, &body)
}

pub(in crate::nodes::ai) async fn embed_ollama(
    client: &reqwest::Client,
    host: &str,
    model: &str,
    texts: &[String],
) -> Result<Vec<Vec<f64>>> {
    let url = format!("{}/api/embed", host.trim_end_matches('/'));
    let response = client
        .post(&url)
        .json(&serde_json::json!({ "model": model, "input": texts }))
        .send()
        .await
        .map_err(|error| anyhow::anyhow!("Ollama embedding request failed: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| anyhow::anyhow!("Failed to read Ollama response: {}", error))?;

    parse_ollama_response(status, &body)
}

pub(super) async fn embed_for_config(
    client: &reqwest::Client,
    config: &serde_json::Value,
    ctx: &Context,
    texts: &[String],
) -> Result<(Vec<Vec<f64>>, String)> {
    match config
        .get("provider")
        .and_then(|value| value.as_str())
        .unwrap_or("openai")
    {
        "openai" => embed_with_openai(client, config, ctx, texts).await,
        "ollama" => embed_with_ollama(client, config, ctx, texts).await,
        "oauth" => embed_with_oauth(client, config, ctx, texts).await,
        provider => anyhow::bail!("ai_embed: unsupported provider '{}'", provider),
    }
}

async fn embed_with_openai(
    client: &reqwest::Client,
    config: &serde_json::Value,
    ctx: &Context,
    texts: &[String],
) -> Result<(Vec<Vec<f64>>, String)> {
    let api_key = resolve_param(config, "api_key", "OPENAI_API_KEY", ctx).ok_or_else(|| {
        anyhow::anyhow!("ai_embed (openai) requires 'api_key' or OPENAI_API_KEY env var")
    })?;
    let base_url = resolve_param(config, "base_url", "OPENAI_BASE_URL", ctx)
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
    let model = resolve_model(config, "text-embedding-3-small");
    let embeddings = embed_openai(client, &base_url, &api_key, &model, texts).await?;
    Ok((embeddings, model))
}

async fn embed_with_ollama(
    client: &reqwest::Client,
    config: &serde_json::Value,
    ctx: &Context,
    texts: &[String],
) -> Result<(Vec<Vec<f64>>, String)> {
    let host = resolve_param(config, "ollama_host", "OLLAMA_HOST", ctx)
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let model = resolve_model(config, "nomic-embed-text");
    let embeddings = embed_ollama(client, &host, &model, texts).await?;
    Ok((embeddings, model))
}

async fn embed_with_oauth(
    client: &reqwest::Client,
    config: &serde_json::Value,
    ctx: &Context,
    texts: &[String],
) -> Result<(Vec<Vec<f64>>, String)> {
    let token_url = required_param(config, "token_url", "OAUTH_TOKEN_URL", ctx, "token_url")?;
    let client_id = required_param(config, "client_id", "OAUTH_CLIENT_ID", ctx, "client_id")?;
    let client_secret = required_param(
        config,
        "client_secret",
        "OAUTH_CLIENT_SECRET",
        ctx,
        "client_secret",
    )?;
    let base_url = required_param(config, "base_url", "OAUTH_BASE_URL", ctx, "base_url")?;
    let scope = resolve_param(config, "scope", "OAUTH_SCOPE", ctx);
    let model = resolve_model(config, "openai-text-embedding-3-small");
    let token = acquire_oauth_token(
        client,
        &token_url,
        &client_id,
        &client_secret,
        scope.as_deref(),
    )
    .await?;
    let embeddings = embed_openai(client, &base_url, &token, &model, texts).await?;
    Ok((embeddings, model))
}

fn required_param(
    config: &serde_json::Value,
    key: &str,
    env_key: &str,
    ctx: &Context,
    label: &str,
) -> Result<String> {
    resolve_param(config, key, env_key, ctx).ok_or_else(|| {
        anyhow::anyhow!(
            "ai_embed (oauth) requires '{}' or {} env var",
            label,
            env_key
        )
    })
}

fn resolve_model(config: &serde_json::Value, default: &str) -> String {
    config
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or(default)
        .to_string()
}
