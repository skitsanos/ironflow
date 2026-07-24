//! Typed readers for node configuration values.
//!
//! Node parameters reach a node as `serde_json::Value`. String parameters are routinely
//! written as `"${ctx.key}"` templates, but `${ctx.key}` interpolation always produces a
//! string — so a numeric parameter written that way arrives as `Value::String`, and a
//! bare `as_f64()` / `as_u64()` read yields `None`. Nodes that fall back to a default on
//! `None` then ignore the caller's value with no error and no warning.
//!
//! These readers accept a native JSON number, a numeric string, or a `${ctx.key}` template
//! resolving to either, so numeric parameters behave like string parameters do.

use anyhow::Result;
use serde_json::Value;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

/// Read a floating-point node parameter, resolving `${ctx.key}` templates.
///
/// Returns `None` when the key is absent or the value cannot be read as a number, which
/// lets callers keep their existing `.unwrap_or(default)` behaviour.
pub fn config_f64(config: &Value, key: &str, ctx: &Context) -> Option<f64> {
    let value = match config.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => interpolate_ctx(s, ctx).trim().parse::<f64>().ok(),
        _ => None,
    }?;
    value.is_finite().then_some(value)
}

/// Read an optional finite floating-point parameter, using `default` only when
/// the key is absent. Present-but-invalid values are rejected instead of being
/// silently mistaken for an omitted setting.
pub fn config_f64_or(config: &Value, key: &str, ctx: &Context, default: f64) -> Result<f64> {
    if config.get(key).is_none() {
        return Ok(default);
    }

    config_f64(config, key, ctx).ok_or_else(|| anyhow::anyhow!("'{key}' must be a finite number"))
}

/// Read an unsigned-integer node parameter, resolving `${ctx.key}` templates.
///
/// Lua has a single number type, so integer parameters routinely arrive as floats
/// (`15.0`); whole floats are accepted. Fractional, negative, and non-finite values are
/// rejected.
pub fn config_u64(config: &Value, key: &str, ctx: &Context) -> Option<u64> {
    match config.get(key)? {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().and_then(f64_to_u64)),
        Value::String(s) => {
            let resolved = interpolate_ctx(s, ctx);
            let resolved = resolved.trim();
            resolved
                .parse::<u64>()
                .ok()
                .or_else(|| resolved.parse::<f64>().ok().and_then(f64_to_u64))
        }
        _ => None,
    }
}

/// Read an optional unsigned integer while distinguishing absence from an
/// invalid present value.
pub fn config_u64_strict(config: &Value, key: &str, ctx: &Context) -> Result<Option<u64>> {
    if config.get(key).is_none() {
        return Ok(None);
    }

    config_u64(config, key, ctx)
        .map(Some)
        .ok_or_else(|| anyhow::anyhow!("'{key}' must be a non-negative whole number in u64 range"))
}

/// Read an optional platform-sized unsigned integer.
///
/// This rejects values that are valid `u64`s but cannot fit into `usize` on the
/// current target instead of allowing a narrowing `as` conversion to wrap.
pub fn config_usize_strict(config: &Value, key: &str, ctx: &Context) -> Result<Option<usize>> {
    config_u64_strict(config, key, ctx)?
        .map(|value| {
            usize::try_from(value)
                .map_err(|_| anyhow::anyhow!("'{key}' exceeds the platform usize range"))
        })
        .transpose()
}

/// Read a boolean node parameter, resolving `${ctx.key}` templates.
///
/// Accepts the same vocabulary as the boolean environment variables elsewhere in the
/// codebase: `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`, case-insensitive.
pub fn config_bool(config: &Value, key: &str, ctx: &Context) -> Option<bool> {
    match config.get(key)? {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match interpolate_ctx(s, ctx).trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Some(true),
            "false" | "no" | "off" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Read an optional boolean parameter, using `default` only when the key is
/// absent. Present-but-invalid values are rejected.
pub fn config_bool_or(config: &Value, key: &str, ctx: &Context, default: bool) -> Result<bool> {
    if config.get(key).is_none() {
        return Ok(default);
    }

    config_bool(config, key, ctx).ok_or_else(|| anyhow::anyhow!("'{key}' must be a boolean"))
}

fn f64_to_u64(value: f64) -> Option<u64> {
    if value.is_finite() && value >= 0.0 && value.trunc() == value && value < u64::MAX as f64 {
        Some(value as u64)
    } else {
        None
    }
}
