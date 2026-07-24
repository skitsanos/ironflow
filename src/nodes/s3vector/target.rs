use anyhow::Result;

use crate::engine::types::Context;

mod source;

use source::{TargetScope, TargetValues, resolve_values};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TargetPolicy {
    AllowEnvironment,
    ExplicitOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BucketTarget {
    Name(String),
    Arn(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CreateIndexTarget {
    pub(super) bucket: BucketTarget,
    pub(super) index_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum IndexTarget {
    Names {
        bucket_name: String,
        index_name: String,
    },
    Arn(String),
}

pub(super) fn resolve_create_bucket_name(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
) -> Result<String> {
    with_process_env(|env| resolve_create_bucket_name_with_env(config, ctx, node, env))
}

pub(super) fn resolve_bucket_target(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
    policy: TargetPolicy,
) -> Result<BucketTarget> {
    with_process_env(|env| resolve_bucket_target_with_env(config, ctx, node, policy, env))
}

pub(super) fn resolve_create_index_target(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
) -> Result<CreateIndexTarget> {
    with_process_env(|env| resolve_create_index_target_with_env(config, ctx, node, env))
}

pub(super) fn resolve_index_target(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
    policy: TargetPolicy,
) -> Result<IndexTarget> {
    with_process_env(|env| resolve_index_target_with_env(config, ctx, node, policy, env))
}

fn with_process_env<T>(
    resolve: impl FnOnce(&dyn Fn(&str) -> Option<String>) -> Result<T>,
) -> Result<T> {
    let env = |key: &str| std::env::var(key).ok();
    resolve(&env)
}

fn resolve_create_bucket_name_with_env(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<String> {
    let values = resolve_values(config, ctx, node, TargetScope::Bucket, true, env)?;
    match (values.bucket_name, values.bucket_arn) {
        (Some(_), Some(_)) => ambiguous_bucket(node),
        (Some(name), None) => Ok(name),
        (None, Some(_)) => anyhow::bail!(
            "{} does not support 'vector_bucket_arn'; use 'vector_bucket_name'",
            node
        ),
        (None, None) => anyhow::bail!("{} requires 'vector_bucket_name'", node),
    }
}

fn resolve_bucket_target_with_env(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
    policy: TargetPolicy,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<BucketTarget> {
    let values = resolve_values(
        config,
        ctx,
        node,
        TargetScope::Bucket,
        matches!(policy, TargetPolicy::AllowEnvironment),
        env,
    )?;
    bucket_target(values, node)
}

fn resolve_create_index_target_with_env(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<CreateIndexTarget> {
    let values = resolve_values(config, ctx, node, TargetScope::Index, true, env)?;
    if values.index_name.is_some() && values.index_arn.is_some() {
        return ambiguous_index(node);
    }
    if values.index_arn.is_some() {
        anyhow::bail!("{} does not support 'index_arn'; use 'index_name'", node);
    }
    let index_name = values
        .index_name
        .clone()
        .ok_or_else(|| anyhow::anyhow!("{} requires 'index_name'", node))?;
    let bucket = bucket_target(values, node)?;
    Ok(CreateIndexTarget { bucket, index_name })
}

fn resolve_index_target_with_env(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
    policy: TargetPolicy,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<IndexTarget> {
    let values = resolve_values(
        config,
        ctx,
        node,
        TargetScope::Index,
        matches!(policy, TargetPolicy::AllowEnvironment),
        env,
    )?;
    if values.index_name.is_some() && values.index_arn.is_some() {
        return ambiguous_index(node);
    }

    match (values.index_name, values.index_arn) {
        (Some(index_name), None) => match (values.bucket_name, values.bucket_arn) {
            (Some(_), Some(_)) => ambiguous_bucket(node),
            (Some(bucket_name), None) => Ok(IndexTarget::Names {
                bucket_name,
                index_name,
            }),
            (None, Some(_)) | (None, None) => anyhow::bail!(
                "{} requires 'vector_bucket_name' when using 'index_name'",
                node
            ),
        },
        (None, Some(index_arn)) => {
            if values.bucket_name.is_some() || values.bucket_arn.is_some() {
                anyhow::bail!(
                    "{} does not accept bucket identifiers when using 'index_arn'",
                    node
                );
            }
            Ok(IndexTarget::Arn(index_arn))
        }
        (None, None) => anyhow::bail!("{} requires 'index_name' or 'index_arn'", node),
        (Some(_), Some(_)) => unreachable!("index ambiguity is handled above"),
    }
}

fn bucket_target(values: TargetValues, node: &str) -> Result<BucketTarget> {
    match (values.bucket_name, values.bucket_arn) {
        (Some(_), Some(_)) => ambiguous_bucket(node),
        (Some(name), None) => Ok(BucketTarget::Name(name)),
        (None, Some(arn)) => Ok(BucketTarget::Arn(arn)),
        (None, None) => anyhow::bail!(
            "{} requires 'vector_bucket_name' or 'vector_bucket_arn'",
            node
        ),
    }
}

fn ambiguous_bucket<T>(node: &str) -> Result<T> {
    anyhow::bail!(
        "{} requires exactly one of 'vector_bucket_name' or 'vector_bucket_arn'",
        node
    )
}

fn ambiguous_index<T>(node: &str) -> Result<T> {
    anyhow::bail!(
        "{} requires exactly one of 'index_name' or 'index_arn'",
        node
    )
}

#[cfg(test)]
mod tests;
