use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

use super::config::{LlmMode, Provider};
use crate::nodes::ai::embeddings::resolve_param;

type ResolvedProvider = (String, HeaderMap, String);

pub(crate) fn resolve_provider_config(
    config: &Value,
    ctx: &Context,
    mode: LlmMode,
) -> Result<ResolvedProvider> {
    let provider = Provider::resolve(config);
    match provider {
        Provider::OpenAI | Provider::OpenAICompatible => {
            resolve_openai_provider(config, ctx, mode, provider)
        }
        Provider::Azure => resolve_azure_provider(config, ctx, mode),
        Provider::Custom => resolve_custom_provider(config, ctx, mode),
    }
}

fn resolve_openai_provider(
    config: &Value,
    ctx: &Context,
    mode: LlmMode,
    provider: Provider,
) -> Result<ResolvedProvider> {
    let base_url = if matches!(provider, Provider::OpenAICompatible) {
        resolve_param(config, "base_url", "OPENAI_COMPATIBLE_BASE_URL", ctx)
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
    let api_key = resolve_param(config, "api_key", "OPENAI_API_KEY", ctx)
        .ok_or_else(|| anyhow::anyhow!("llm (openai) requires 'api_key' or OPENAI_API_KEY"))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("authorization"),
        HeaderValue::from_str(&format!("Bearer {api_key}"))?,
    );

    let path = match mode {
        LlmMode::Chat => "chat/completions",
        LlmMode::Responses => "responses",
    };
    Ok((
        format!("{}/{path}", base_url.trim_end_matches('/')),
        headers,
        provider.name().to_string(),
    ))
}

fn resolve_azure_provider(
    config: &Value,
    ctx: &Context,
    mode: LlmMode,
) -> Result<ResolvedProvider> {
    let endpoint = resolve_param(config, "azure_endpoint", "AZURE_OPENAI_ENDPOINT", ctx)
        .ok_or_else(|| {
            anyhow::anyhow!("llm (azure) requires 'azure_endpoint' or AZURE_OPENAI_ENDPOINT")
        })?;
    let api_version = resolve_param(config, "azure_api_version", "AZURE_OPENAI_API_VERSION", ctx)
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
    let deployment = match mode {
        LlmMode::Chat => chat_deployment.or_else(|| responses_deployment.clone()),
        LlmMode::Responses => responses_deployment.or_else(|| chat_deployment.clone()),
    }
    .ok_or_else(|| anyhow::anyhow!("llm (azure) requires deployment for selected mode"))?;
    let api_key = resolve_param(config, "api_key", "AZURE_OPENAI_API_KEY", ctx)
        .ok_or_else(|| anyhow::anyhow!("llm (azure) requires 'api_key' or AZURE_OPENAI_API_KEY"))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("api-key"),
        HeaderValue::from_str(&api_key)?,
    );
    let path = match mode {
        LlmMode::Chat => "chat/completions",
        LlmMode::Responses => "responses",
    };

    Ok((
        format!(
            "{}/openai/deployments/{}/{}?api-version={}",
            endpoint.trim_end_matches('/'),
            deployment,
            path,
            api_version
        ),
        headers,
        Provider::Azure.name().to_string(),
    ))
}

fn resolve_custom_provider(
    config: &Value,
    ctx: &Context,
    mode: LlmMode,
) -> Result<ResolvedProvider> {
    let base_url = resolve_param(config, "base_url", "LLM_BASE_URL", ctx).ok_or_else(|| {
        anyhow::anyhow!(
            "llm (custom) requires 'base_url' or LLM_BASE_URL when using custom provider"
        )
    })?;
    let default_path = match mode {
        LlmMode::Chat => "/chat/completions",
        LlmMode::Responses => "/responses",
    };
    let mode_path_key = match mode {
        LlmMode::Chat => "chat_path",
        LlmMode::Responses => "responses_path",
    };
    let path = config
        .get(mode_path_key)
        .or_else(|| config.get("path"))
        .and_then(Value::as_str)
        .unwrap_or(default_path);
    let endpoint = resolve_custom_endpoint(&base_url, path);
    let headers = resolve_custom_auth(config, ctx)?;

    Ok((endpoint, headers, Provider::Custom.name().to_string()))
}

fn resolve_custom_endpoint(base_url: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }

    format!(
        "{}{}{}",
        base_url.trim_end_matches('/'),
        if path.starts_with('/') { "" } else { "/" },
        path
    )
}

fn resolve_custom_auth(config: &Value, ctx: &Context) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    let auth_type = config
        .get("auth_type")
        .and_then(Value::as_str)
        .unwrap_or("bearer")
        .to_ascii_lowercase();

    match auth_type.as_str() {
        "none" => {}
        "bearer" => {
            if let Some(token) = config.get("api_key").and_then(Value::as_str) {
                headers.insert(
                    HeaderName::from_static("authorization"),
                    HeaderValue::from_str(&format!("Bearer {}", interpolate_ctx(token, ctx)))?,
                );
            }
        }
        "api_key" => {
            if let Some(api_key) = config.get("api_key").and_then(Value::as_str) {
                let header_name = config
                    .get("auth_header")
                    .and_then(Value::as_str)
                    .unwrap_or("x-api-key")
                    .to_lowercase();
                headers.insert(
                    HeaderName::from_bytes(header_name.as_bytes())?,
                    HeaderValue::from_str(&interpolate_ctx(api_key, ctx))?,
                );
            }
        }
        other => anyhow::bail!(
            "llm: unsupported auth_type '{}' for custom provider; use 'bearer', 'api_key', or 'none'",
            other
        ),
    }
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openai_compatible_routes_responses_and_sets_bearer_auth() {
        let config = json!({
            "provider": "openai_compatible",
            "base_url": "https://gateway.example/v1/",
            "api_key": "compatible-secret",
        });

        let (endpoint, headers, provider) =
            resolve_provider_config(&config, &Context::new(), LlmMode::Responses).unwrap();

        assert_eq!(endpoint, "https://gateway.example/v1/responses");
        assert_eq!(provider, "openai_compatible");
        assert_eq!(
            headers
                .get(reqwest::header::AUTHORIZATION)
                .unwrap()
                .to_str()
                .unwrap(),
            "Bearer compatible-secret"
        );
    }

    #[test]
    fn azure_routes_each_mode_to_its_deployment_and_path() {
        let config = json!({
            "provider": "azure",
            "azure_endpoint": "https://account.openai.azure.com/",
            "azure_api_version": "2026-01-01-preview",
            "azure_chat_deployment": "chat-deployment",
            "azure_responses_deployment": "responses-deployment",
            "api_key": "azure-secret",
        });
        let cases = [
            (LlmMode::Chat, "chat-deployment/chat/completions"),
            (LlmMode::Responses, "responses-deployment/responses"),
        ];

        for (mode, expected_suffix) in cases {
            let (endpoint, headers, provider) =
                resolve_provider_config(&config, &Context::new(), mode).unwrap();
            assert_eq!(
                endpoint,
                format!(
                    "https://account.openai.azure.com/openai/deployments/{expected_suffix}?api-version=2026-01-01-preview"
                )
            );
            assert_eq!(provider, "azure");
            assert_eq!(headers.get("api-key").unwrap(), "azure-secret");
        }
    }
}
