use anyhow::Result;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

pub(super) fn resolve_optional(
    config: &serde_json::Value,
    keys: &[&str],
    env_keys: &[&str],
    ctx: &Context,
) -> Option<String> {
    keys.iter()
        .find_map(|key| {
            config
                .get(key)
                .and_then(|value| value.as_str())
                .map(|value| interpolate_ctx(value, ctx))
        })
        .or_else(|| env_keys.iter().find_map(|key| std::env::var(key).ok()))
}

pub(super) fn resolve_output_key(config: &serde_json::Value) -> String {
    config
        .get("output_key")
        .and_then(|value| value.as_str())
        .unwrap_or("s3vector")
        .to_string()
}

/// A setting that decides where requests go. The environment is consulted
/// only when the parameter is absent: a configured value that is not a
/// non-empty string fails instead of sending the request to the default
/// region or endpoint (IF-221).
fn resolve_routing(
    config: &serde_json::Value,
    key: &str,
    env_keys: &[&str],
    ctx: &Context,
) -> Result<Option<String>> {
    match config.get(key) {
        None => Ok(env_keys.iter().find_map(|key| std::env::var(key).ok())),
        Some(serde_json::Value::String(value)) => {
            let value = interpolate_ctx(value, ctx);
            if value.trim().is_empty() {
                anyhow::bail!("s3vector requires '{key}' to be non-empty");
            }
            Ok(Some(value))
        }
        Some(_) => anyhow::bail!("s3vector requires '{key}' to be a string"),
    }
}

pub(super) fn resolve_region(config: &serde_json::Value, ctx: &Context) -> Result<Option<String>> {
    resolve_routing(
        config,
        "region",
        &[
            "S3VECTORS_REGION",
            "S3_REGION",
            "AWS_REGION",
            "AWS_DEFAULT_REGION",
        ],
        ctx,
    )
}

pub(super) fn resolve_endpoint_url(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<Option<String>> {
    resolve_routing(config, "endpoint_url", &["AWS_ENDPOINT_URL"], ctx)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::client::build_s3vector_client;
    use super::*;

    /// Building a client is local: nothing is sent until a request is made.
    async fn client_error(config: serde_json::Value, ctx: &Context) -> String {
        match build_s3vector_client(&config, ctx).await {
            Ok(_) => panic!("a client was built from {config}"),
            Err(error) => error.to_string(),
        }
    }

    #[tokio::test]
    async fn a_malformed_endpoint_or_region_builds_no_client() {
        let ctx = Context::from([("blank".to_owned(), json!("  "))]);
        let local = "http://127.0.0.1:1";
        for (config, expected) in [
            (
                json!({ "endpoint_url": 8080, "region": "us-east-1" }),
                "s3vector requires 'endpoint_url' to be a string",
            ),
            (
                json!({ "endpoint_url": { "url": local }, "region": "us-east-1" }),
                "s3vector requires 'endpoint_url' to be a string",
            ),
            (
                json!({ "endpoint_url": "${ctx.absent}", "region": "us-east-1" }),
                "s3vector requires 'endpoint_url' to be non-empty",
            ),
            (
                json!({ "endpoint_url": "${ctx.blank}", "region": "us-east-1" }),
                "s3vector requires 'endpoint_url' to be non-empty",
            ),
            (
                json!({ "endpoint_url": local, "region": 1 }),
                "s3vector requires 'region' to be a string",
            ),
            (
                json!({ "endpoint_url": local, "region": "${ctx.absent}" }),
                "s3vector requires 'region' to be non-empty",
            ),
        ] {
            assert_eq!(client_error(config, &ctx).await, expected);
        }
    }

    #[tokio::test]
    async fn a_configured_endpoint_and_region_are_interpolated() {
        let ctx = Context::from([("port".to_owned(), json!(1))]);
        let config =
            json!({ "endpoint_url": "http://127.0.0.1:${ctx.port}", "region": "us-east-1" });
        assert_eq!(
            resolve_endpoint_url(&config, &ctx).unwrap().as_deref(),
            Some("http://127.0.0.1:1")
        );
        assert_eq!(
            resolve_region(&config, &ctx).unwrap().as_deref(),
            Some("us-east-1")
        );
        assert!(build_s3vector_client(&config, &ctx).await.is_ok());
    }
}
