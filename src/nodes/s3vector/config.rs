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

pub(super) fn resolve_region(config: &serde_json::Value, ctx: &Context) -> Option<String> {
    resolve_optional(
        config,
        &["region"],
        &[
            "S3VECTORS_REGION",
            "S3_REGION",
            "AWS_REGION",
            "AWS_DEFAULT_REGION",
        ],
        ctx,
    )
}

pub(super) fn resolve_endpoint_url(config: &serde_json::Value, ctx: &Context) -> Option<String> {
    resolve_optional(config, &["endpoint_url"], &["AWS_ENDPOINT_URL"], ctx)
}
