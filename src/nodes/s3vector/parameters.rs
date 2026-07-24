use anyhow::Result;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;
use crate::util::node_config::config_f64;

use super::config::{resolve_optional, resolve_required};

pub(super) fn resolve_i64(
    config: &serde_json::Value,
    keys: &[&str],
    ctx: &Context,
    node: &str,
    field: &str,
) -> Result<i64> {
    let mut value = config.get(keys[0]);
    if value.is_none() {
        for key in &keys[1..] {
            if let Some(candidate) = config.get(key) {
                value = Some(candidate);
                break;
            }
        }
    }
    if let Some(value) = value {
        resolve_i64_value(value, ctx)
            .ok_or_else(|| anyhow::anyhow!("{} requires '{}' as an integer", node, field))
    } else {
        Err(anyhow::anyhow!("{} requires '{}' field", node, field))
    }
}

fn resolve_i64_value(value: &serde_json::Value, ctx: &Context) -> Option<i64> {
    match value {
        serde_json::Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| number.as_f64().and_then(f64_to_i64)),
        serde_json::Value::String(text) => {
            let resolved = interpolate_ctx(text, ctx);
            let resolved = resolved.trim();
            resolved
                .parse::<i64>()
                .ok()
                .or_else(|| {
                    resolved
                        .parse::<u64>()
                        .ok()
                        .and_then(|value| i64::try_from(value).ok())
                })
                .or_else(|| resolved.parse::<f64>().ok().and_then(f64_to_i64))
        }
        _ => None,
    }
}

fn f64_to_i64(value: f64) -> Option<i64> {
    if value.is_finite()
        && value.trunc() == value
        && value >= i64::MIN as f64
        && value < i64::MAX as f64
    {
        Some(value as i64)
    } else {
        None
    }
}

pub(super) fn resolve_u32(
    config: &serde_json::Value,
    keys: &[&str],
    ctx: &Context,
    node: &str,
    field: &str,
) -> Result<u32> {
    resolve_i64(config, keys, ctx, node, field).and_then(|value| {
        u32::try_from(value)
            .map_err(|_| anyhow::anyhow!("{} requires '{}' as a non-negative integer", node, field))
    })
}

pub(super) fn resolve_f64(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
    field: &str,
) -> Result<f64> {
    config
        .get(field)
        .ok_or_else(|| anyhow::anyhow!("{} requires '{}' field", node, field))?;

    let number = config_f64(config, field, ctx)
        .ok_or_else(|| anyhow::anyhow!("{} requires '{}' to be a number", node, field))?;

    if !number.is_finite() {
        anyhow::bail!("{} requires '{}' to be a finite number", node, field);
    }

    Ok(number)
}

pub(super) fn resolve_non_empty_string(
    config: &serde_json::Value,
    keys: &[&str],
    env_keys: &[&str],
    ctx: &Context,
    node: &str,
    field: &str,
) -> Result<String> {
    let value = resolve_required(config, keys, env_keys, ctx, node, field)?;
    if value.is_empty() {
        anyhow::bail!("{} requires '{}' to be non-empty", node, field);
    }
    Ok(value)
}

pub(super) fn resolve_string_array(
    config: &serde_json::Value,
    primary_key: &str,
    fallback_key: Option<&str>,
    ctx: &Context,
    node: &str,
    field: &str,
) -> Result<Vec<String>> {
    let raw = if let Some(values) = config.get(primary_key) {
        values
    } else if let Some(fallback_key) = fallback_key {
        let fallback = resolve_optional(config, &[fallback_key], &[], ctx)
            .ok_or_else(|| anyhow::anyhow!("{} requires '{}'", node, field))?;
        ctx.get(&fallback).ok_or_else(|| {
            anyhow::anyhow!(
                "{} requires '{}' source key '{}' to exist in context",
                node,
                field,
                fallback
            )
        })?
    } else {
        return Err(anyhow::anyhow!("{} requires '{}'", node, field));
    };

    let raw = raw
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{} requires '{}' to be an array", node, field))?;

    let mut values = Vec::new();
    for value in raw {
        let item = value.as_str().ok_or_else(|| {
            anyhow::anyhow!("{} requires each '{}' item to be a string", node, field)
        })?;
        values.push(interpolate_ctx(item, ctx));
    }

    if values.is_empty() {
        anyhow::bail!("{} requires '{}' to be a non-empty array", node, field);
    }

    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx_with(key: &str, value: serde_json::Value) -> Context {
        let mut ctx = Context::new();
        ctx.insert(key.to_string(), value);
        ctx
    }

    #[test]
    fn resolve_u32_accepts_interpolated_integer() {
        let ctx = ctx_with("top_k", json!(3));
        let config = json!({ "top_k": "${ctx.top_k}" });

        assert_eq!(
            resolve_u32(&config, &["top_k"], &ctx, "node", "top_k").unwrap(),
            3
        );
    }

    #[test]
    fn resolve_u32_rejects_interpolated_fraction() {
        let ctx = ctx_with("top_k", json!(3.5));
        let config = json!({ "top_k": "${ctx.top_k}" });

        let error = resolve_u32(&config, &["top_k"], &ctx, "node", "top_k").unwrap_err();
        assert!(error.to_string().contains("requires 'top_k' as an integer"));
    }

    #[test]
    fn resolve_f64_accepts_interpolated_number() {
        let ctx = ctx_with("min_similarity", json!("0.7"));
        let config = json!({ "min_similarity": "${ctx.min_similarity}" });

        assert_eq!(
            resolve_f64(&config, &ctx, "node", "min_similarity").unwrap(),
            0.7
        );
    }
}
