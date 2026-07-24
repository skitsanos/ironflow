use anyhow::Result;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::Region;
use base64::Engine;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;
use crate::util::node_config::config_u64;

pub(super) fn resolve_required(
    config: &serde_json::Value,
    key: &str,
    env_key: Option<&str>,
    ctx: &Context,
) -> Option<String> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| interpolate_ctx(value, ctx))
        .or_else(|| env_key.and_then(|name| std::env::var(name).ok()))
}

pub(super) fn resolve_optional(
    config: &serde_json::Value,
    key: &str,
    env_key: Option<&str>,
    ctx: &Context,
) -> Option<String> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| interpolate_ctx(value, ctx))
        .or_else(|| env_key.and_then(|name| std::env::var(name).ok()))
}

pub(super) fn resolve_output_key(config: &serde_json::Value) -> String {
    config
        .get("output_key")
        .and_then(|value| value.as_str())
        .unwrap_or("s3")
        .to_string()
}

pub(super) fn resolve_bool(
    config: &serde_json::Value,
    key: &str,
    env_key: Option<&str>,
    ctx: &Context,
) -> bool {
    crate::util::node_config::config_bool(config, key, ctx)
        .or_else(|| env_key.and_then(parse_bool_env))
        .unwrap_or(false)
}

fn parse_bool_env(name: &str) -> Option<bool> {
    std::env::var(name)
        .ok()
        .and_then(|value| match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
}

pub(super) fn resolve_region(config: &serde_json::Value, ctx: &Context) -> Option<String> {
    resolve_optional(config, "region", Some("S3_REGION"), ctx)
        .or_else(|| std::env::var("AWS_REGION").ok())
        .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
}

pub(super) fn resolve_expires_in(config: &serde_json::Value, ctx: &Context) -> Result<u64> {
    let expires_in = config_u64(config, "expires_in", ctx).unwrap_or(3600);
    if expires_in == 0 || expires_in > 604800 {
        anyhow::bail!("s3_presign_url requires 'expires_in' to be between 1 and 604800 seconds");
    }
    Ok(expires_in)
}

pub(super) fn resolve_content_length(config: &serde_json::Value, ctx: &Context) -> Option<i64> {
    config_u64(config, "content_length", ctx)
        .or_else(|| config_u64(config, "contentLength", ctx))
        .and_then(|value| i64::try_from(value).ok())
        .filter(|value| *value > 0)
}

pub(super) async fn build_s3_client(config: &serde_json::Value, ctx: &Context) -> Result<Client> {
    let force_path_style = resolve_bool(
        config,
        "force_path_style",
        Some("AWS_S3_FORCE_PATH_STYLE"),
        ctx,
    );
    let endpoint_url = resolve_optional(config, "endpoint_url", Some("AWS_ENDPOINT_URL"), ctx);
    let region = resolve_region(config, ctx);

    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(region) = region {
        loader = loader.region(Region::new(region));
    }

    let base_config = loader.load().await;
    let mut s3_builder = aws_sdk_s3::config::Builder::from(&base_config);
    s3_builder = s3_builder.force_path_style(force_path_style);
    if let Some(endpoint) = endpoint_url {
        s3_builder = s3_builder.endpoint_url(endpoint);
    }
    Ok(Client::from_conf(s3_builder.build()))
}

pub(super) async fn resolve_payload_bytes(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<Vec<u8>> {
    let encoding = config
        .get("encoding")
        .or_else(|| config.get("source_encoding"))
        .and_then(|value| value.as_str())
        .unwrap_or("text");

    if let Some(source_path) = config.get("source_path").and_then(|value| value.as_str()) {
        let source_path = interpolate_ctx(source_path, ctx);
        return Ok(tokio::fs::read(source_path).await?);
    }

    if let Some(source_key) = config.get("source_key").and_then(|value| value.as_str()) {
        let source_key = interpolate_ctx(source_key, ctx);
        let raw = ctx
            .get(&source_key)
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!("source_key '{}' must be a string in context", source_key)
            })?;
        return match encoding {
            "base64" => base64::engine::general_purpose::STANDARD
                .decode(raw)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "Failed to decode base64 source_key '{}': {}",
                        source_key,
                        error
                    )
                }),
            "text" => Ok(raw.as_bytes().to_vec()),
            other => anyhow::bail!(
                "s3 payload encoding '{}' is not supported. Use 'text' or 'base64'",
                other
            ),
        };
    }

    if let Some(content) = config.get("content").and_then(|value| value.as_str()) {
        let content = interpolate_ctx(content, ctx);
        return match encoding {
            "text" => Ok(content.as_bytes().to_vec()),
            "base64" => base64::engine::general_purpose::STANDARD
                .decode(content)
                .map_err(|error| anyhow::anyhow!("Failed to decode base64 content: {}", error)),
            other => anyhow::bail!(
                "s3 payload encoding '{}' is not supported. Use 'text' or 'base64'",
                other
            ),
        };
    }

    anyhow::bail!("S3 node requires one of 'content', 'source_key', or 'source_path'");
}

pub(super) fn write_payload_to_output(
    output: &mut crate::engine::types::NodeOutput,
    output_key: &str,
    bytes: &[u8],
    encoding: &str,
) -> Result<()> {
    let body_value = match encoding {
        "base64" => {
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        "text" => {
            let value = String::from_utf8(bytes.to_vec()).map_err(|error| {
                anyhow::anyhow!("Failed to convert response body to UTF-8: {}", error)
            })?;
            serde_json::Value::String(value)
        }
        other => {
            return Err(anyhow::anyhow!(
                "s3 payload encoding '{}' is not supported. Use 'text' or 'base64'",
                other
            ));
        }
    };

    output.insert(format!("{}_content", output_key), body_value);
    output.insert(
        format!("{}_encoding", output_key),
        serde_json::Value::String(encoding.to_string()),
    );
    output.insert(
        format!("{}_size", output_key),
        serde_json::Value::Number(bytes.len().into()),
    );
    Ok(())
}
