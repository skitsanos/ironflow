use anyhow::Result;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

use super::config::{resolve_optional, resolve_required};

pub(super) fn resolve_i64(
    config: &serde_json::Value,
    keys: &[&str],
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
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| {
                value.as_f64().and_then(|value| {
                    if (value.trunc() == value) && value.is_finite() {
                        Some(value as i64)
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| anyhow::anyhow!("{} requires '{}' as an integer", node, field))
    } else {
        Err(anyhow::anyhow!("{} requires '{}' field", node, field))
    }
}

pub(super) fn resolve_u32(
    config: &serde_json::Value,
    keys: &[&str],
    node: &str,
    field: &str,
) -> Result<u32> {
    resolve_i64(config, keys, node, field).and_then(|value| {
        u32::try_from(value)
            .map_err(|_| anyhow::anyhow!("{} requires '{}' as a non-negative integer", node, field))
    })
}

pub(super) fn resolve_f64(config: &serde_json::Value, node: &str, field: &str) -> Result<f64> {
    let value = config
        .get(field)
        .ok_or_else(|| anyhow::anyhow!("{} requires '{}' field", node, field))?;

    let number = if let Some(value) = value.as_f64() {
        value
    } else if let Some(value) = value.as_i64() {
        value as f64
    } else if let Some(value) = value.as_u64() {
        value as f64
    } else {
        return Err(anyhow::anyhow!(
            "{} requires '{}' to be a number",
            node,
            field
        ));
    };

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
