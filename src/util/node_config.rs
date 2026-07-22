//! Typed readers for node configuration values.
//!
//! Node parameters reach a node as `serde_json::Value`. String parameters are routinely
//! written as `"${ctx.key}"` templates, but `${ctx.*}` interpolation always produces a
//! string — so a numeric parameter written that way arrives as `Value::String`, and a
//! bare `as_f64()` / `as_u64()` read yields `None`. Nodes that fall back to a default on
//! `None` then ignore the caller's value with no error and no warning.
//!
//! These readers accept a native JSON number, a numeric string, or a `${ctx.*}` template
//! resolving to either, so numeric parameters behave like string parameters do.

use serde_json::Value;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

/// Read a floating-point node parameter, resolving `${ctx.*}` templates.
///
/// Returns `None` when the key is absent or the value cannot be read as a number, which
/// lets callers keep their existing `.unwrap_or(default)` behaviour.
pub fn config_f64(config: &Value, key: &str, ctx: &Context) -> Option<f64> {
    match config.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => interpolate_ctx(s, ctx).trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Read an unsigned-integer node parameter, resolving `${ctx.*}` templates.
///
/// Lua has a single number type, so integer parameters routinely arrive as floats
/// (`15.0`); whole floats are accepted and truncated. Negative and non-finite values are
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

/// Read a boolean node parameter, resolving `${ctx.*}` templates.
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

fn f64_to_u64(value: f64) -> Option<u64> {
    if value.is_finite() && value >= 0.0 {
        Some(value as u64)
    } else {
        None
    }
}
