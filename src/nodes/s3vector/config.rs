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

pub(super) fn resolve_required(
    config: &serde_json::Value,
    keys: &[&str],
    env_keys: &[&str],
    ctx: &Context,
    node: &str,
    field: &str,
) -> Result<String> {
    resolve_optional(config, keys, env_keys, ctx).ok_or_else(|| {
        let env_description = env_keys
            .first()
            .map(|value| format!(" or {} env var", value))
            .unwrap_or_default();
        anyhow::anyhow!("{} requires '{}'{}", node, field, env_description)
    })
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

pub(super) fn resolve_bucket_id(
    config: &serde_json::Value,
    ctx: &Context,
    _node: &str,
) -> Result<(Option<String>, Option<String>)> {
    let name = resolve_optional(
        config,
        &["vector_bucket_name", "bucket"],
        &["S3VECTOR_BUCKET_NAME", "S3_BUCKET"],
        ctx,
    );
    let arn = resolve_optional(
        config,
        &["vector_bucket_arn"],
        &["S3VECTOR_BUCKET_ARN"],
        ctx,
    );
    Ok((name, arn))
}

pub(super) fn resolve_index_id(
    config: &serde_json::Value,
    ctx: &Context,
    _node: &str,
) -> Result<(Option<String>, Option<String>)> {
    let name = resolve_optional(
        config,
        &["index_name", "index"],
        &["S3VECTOR_INDEX_NAME"],
        ctx,
    );
    let arn = resolve_optional(config, &["index_arn"], &["S3VECTOR_INDEX_ARN"], ctx);
    Ok((name, arn))
}
