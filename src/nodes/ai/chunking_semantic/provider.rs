use anyhow::Result;

use crate::engine::types::Context;

use crate::nodes::ai::embeddings::{
    acquire_oauth_token, embed_ollama, embed_openai, resolve_param,
};

pub(super) async fn embed_sentences(
    client: &reqwest::Client,
    config: &serde_json::Value,
    ctx: &Context,
    sentences: &[String],
) -> Result<Vec<Vec<f64>>> {
    match config
        .get("provider")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("openai")
    {
        "openai" => embed_with_openai(client, config, ctx, sentences).await,
        "ollama" => embed_with_ollama(client, config, ctx, sentences).await,
        "oauth" => embed_with_oauth(client, config, ctx, sentences).await,
        provider => anyhow::bail!("ai_chunk_semantic: unsupported provider '{}'", provider),
    }
}

async fn embed_with_openai(
    client: &reqwest::Client,
    config: &serde_json::Value,
    ctx: &Context,
    sentences: &[String],
) -> Result<Vec<Vec<f64>>> {
    let api_key = resolve_param(config, "api_key", "OPENAI_API_KEY", ctx).ok_or_else(|| {
        anyhow::anyhow!("ai_chunk_semantic (openai) requires 'api_key' or OPENAI_API_KEY env var")
    })?;
    let base_url = resolve_param(config, "base_url", "OPENAI_BASE_URL", ctx)
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
    let model = resolve_model(config, "text-embedding-3-small");
    embed_openai(client, &base_url, &api_key, model, sentences).await
}

async fn embed_with_ollama(
    client: &reqwest::Client,
    config: &serde_json::Value,
    ctx: &Context,
    sentences: &[String],
) -> Result<Vec<Vec<f64>>> {
    let host = resolve_param(config, "ollama_host", "OLLAMA_HOST", ctx)
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let model = resolve_model(config, "nomic-embed-text");
    embed_ollama(client, &host, model, sentences).await
}

async fn embed_with_oauth(
    client: &reqwest::Client,
    config: &serde_json::Value,
    ctx: &Context,
    sentences: &[String],
) -> Result<Vec<Vec<f64>>> {
    let token_url = required_param(config, "token_url", "OAUTH_TOKEN_URL", ctx)?;
    let client_id = required_param(config, "client_id", "OAUTH_CLIENT_ID", ctx)?;
    let client_secret = required_param(config, "client_secret", "OAUTH_CLIENT_SECRET", ctx)?;
    let base_url = required_param(config, "base_url", "OAUTH_BASE_URL", ctx)?;
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

    embed_openai(client, &base_url, &token, model, sentences).await
}

fn required_param(
    config: &serde_json::Value,
    key: &str,
    env_key: &str,
    ctx: &Context,
) -> Result<String> {
    resolve_param(config, key, env_key, ctx).ok_or_else(|| {
        anyhow::anyhow!(
            "ai_chunk_semantic (oauth) requires '{}' or {} env var",
            key,
            env_key
        )
    })
}

fn resolve_model<'a>(config: &'a serde_json::Value, default: &'a str) -> &'a str {
    config
        .get("model")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(default)
}
