use anyhow::Result;

use crate::engine::types::Context;
use crate::lua::interpolate::try_interpolate_ctx;

const BUCKET_NAME_CONFIG: &[&str] = &["vector_bucket_name", "bucket"];
const BUCKET_ARN_CONFIG: &[&str] = &["vector_bucket_arn"];
const INDEX_NAME_CONFIG: &[&str] = &["index_name", "index"];
const INDEX_ARN_CONFIG: &[&str] = &["index_arn"];

const BUCKET_NAME_ENV: &[&str] = &["S3VECTOR_BUCKET_NAME", "S3_BUCKET"];
const BUCKET_ARN_ENV: &[&str] = &["S3VECTOR_BUCKET_ARN"];
const INDEX_NAME_ENV: &[&str] = &["S3VECTOR_INDEX_NAME"];
const INDEX_ARN_ENV: &[&str] = &["S3VECTOR_INDEX_ARN"];

#[derive(Clone, Copy)]
pub(super) enum TargetScope {
    Bucket,
    Index,
}

#[derive(Default)]
pub(super) struct TargetValues {
    pub(super) bucket_name: Option<String>,
    pub(super) bucket_arn: Option<String>,
    pub(super) index_name: Option<String>,
    pub(super) index_arn: Option<String>,
}

pub(super) fn resolve_values(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
    scope: TargetScope,
    allow_environment: bool,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<TargetValues> {
    if has_configured_target(config, scope) {
        return resolve_config_values(config, ctx, node, scope);
    }
    if allow_environment {
        return resolve_environment_values(node, scope, env);
    }
    Ok(TargetValues::default())
}

fn has_configured_target(config: &serde_json::Value, scope: TargetScope) -> bool {
    let bucket_configured = BUCKET_NAME_CONFIG
        .iter()
        .chain(BUCKET_ARN_CONFIG)
        .any(|key| config.get(*key).is_some());
    match scope {
        TargetScope::Bucket => bucket_configured,
        TargetScope::Index => {
            bucket_configured
                || INDEX_NAME_CONFIG
                    .iter()
                    .chain(INDEX_ARN_CONFIG)
                    .any(|key| config.get(*key).is_some())
        }
    }
}

fn resolve_config_values(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
    scope: TargetScope,
) -> Result<TargetValues> {
    let mut values = TargetValues {
        bucket_name: config_alias(config, BUCKET_NAME_CONFIG, ctx, node)?,
        bucket_arn: config_alias(config, BUCKET_ARN_CONFIG, ctx, node)?,
        ..TargetValues::default()
    };
    if matches!(scope, TargetScope::Index) {
        values.index_name = config_alias(config, INDEX_NAME_CONFIG, ctx, node)?;
        values.index_arn = config_alias(config, INDEX_ARN_CONFIG, ctx, node)?;
    }
    Ok(values)
}

fn resolve_environment_values(
    node: &str,
    scope: TargetScope,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<TargetValues> {
    let mut values = TargetValues {
        // S3_BUCKET is retained as the legacy fallback after the service-specific name.
        bucket_name: environment_alias(BUCKET_NAME_ENV, node, env)?,
        bucket_arn: environment_alias(BUCKET_ARN_ENV, node, env)?,
        ..TargetValues::default()
    };
    if matches!(scope, TargetScope::Index) {
        values.index_name = environment_alias(INDEX_NAME_ENV, node, env)?;
        values.index_arn = environment_alias(INDEX_ARN_ENV, node, env)?;
    }
    Ok(values)
}

fn config_alias(
    config: &serde_json::Value,
    keys: &[&str],
    ctx: &Context,
    node: &str,
) -> Result<Option<String>> {
    let mut selected: Option<(&str, String)> = None;
    for &key in keys {
        let Some(raw) = config.get(key) else {
            continue;
        };
        let raw = raw
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{} requires '{}' to be a string", node, key))?;
        let value = try_interpolate_ctx(raw, ctx).map_err(|error| {
            anyhow::anyhow!("{} could not interpolate '{}': {}", node, key, error)
        })?;
        require_non_blank(&value, node, key)?;

        if let Some((selected_key, selected_value)) = &selected
            && selected_value != &value
        {
            anyhow::bail!(
                "{} requires '{}' and '{}' to resolve to the same value",
                node,
                selected_key,
                key
            );
        }
        selected.get_or_insert((key, value));
    }
    Ok(selected.map(|(_, value)| value))
}

fn environment_alias(
    keys: &[&str],
    node: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<String>> {
    for &key in keys {
        let Some(value) = env(key) else {
            continue;
        };
        if value.trim().is_empty() {
            anyhow::bail!("{} requires {} env var to be non-empty", node, key);
        }
        return Ok(Some(value));
    }
    Ok(None)
}

fn require_non_blank(value: &str, node: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{} requires '{}' to be non-empty", node, field);
    }
    Ok(())
}
