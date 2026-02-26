use anyhow::Result;
use base64::Engine;
use chrono::Utc;
use mlua::prelude::*;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::engine::types::Context;
use crate::nodes::builtin::code_node::{json_value_to_lua_table, lua_value_to_json};

/// Set up the sandboxed Lua environment with standard globals.
///
/// - Removes `os`, `io`, `debug`, `loadfile`, `dofile`
/// - Exposes `env(key)` for reading environment variables
/// - Exposes `base64_encode(str)` and `base64_decode(str)`
/// - Exposes the workflow `ctx` table
///
/// Returns the `ctx` Lua value for callers that need it.
pub fn setup_sandbox(lua: &Lua, ctx: &Context) -> Result<LuaValue> {
    let globals = lua.globals();

    // Remove dangerous modules
    for name in &["os", "io", "debug", "loadfile", "dofile"] {
        globals.set(*name, LuaValue::Nil)?;
    }

    // env(key) -> string | nil
    let env_fn = lua.create_function(|lua_ctx, key: String| match std::env::var(&key) {
        Ok(val) => Ok(LuaValue::String(lua_ctx.create_string(&val)?)),
        Err(_) => Ok(LuaValue::Nil),
    })?;
    globals.set("env", env_fn)?;

    // base64_encode(str) -> string
    let encode_fn = lua.create_function(|lua_ctx, data: LuaString| {
        let encoded = base64::engine::general_purpose::STANDARD.encode(data.as_bytes());
        Ok(LuaValue::String(lua_ctx.create_string(&encoded)?))
    })?;
    globals.set("base64_encode", encode_fn)?;

    // base64_decode(str) -> string
    let decode_fn = lua.create_function(|lua_ctx, data: String| {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&data)
            .map_err(|e| LuaError::RuntimeError(format!("base64_decode failed: {}", e)))?;
        Ok(LuaValue::String(lua_ctx.create_string(&bytes)?))
    })?;
    globals.set("base64_decode", decode_fn)?;

    // json_parse(str) -> Lua table
    let parse_fn = lua.create_function(|lua_ctx, data: String| {
        let json: serde_json::Value = serde_json::from_str(&data)
            .map_err(|e| LuaError::RuntimeError(format!("json_parse failed: {}", e)))?;
        json_value_to_lua_table(lua_ctx, &json).map_err(LuaError::external)
    })?;
    globals.set("json_parse", parse_fn)?;

    // json_stringify(value) -> string
    let stringify_fn = lua.create_function(|_, value: LuaValue| {
        let json_val = lua_value_to_json(&value).map_err(LuaError::external)?;
        let serialized =
            serde_json::to_string(&json_val).map_err(|e| LuaError::RuntimeError(e.to_string()))?;
        Ok(serialized)
    })?;
    globals.set("json_stringify", stringify_fn)?;

    // log([level], message...)
    let log_fn = lua.create_function(|_lua_ctx, args: LuaMultiValue| {
        let values = args.into_iter().collect::<Vec<LuaValue>>();
        if values.is_empty() {
            return Err(LuaError::RuntimeError(
                "log() requires at least one argument".into(),
            ));
        }

        let (level, start_idx) = match values.first().and_then(|v| v.as_string()) {
            Some(level) => {
                let lower = level.to_str()?.to_lowercase();
                if matches!(
                    lower.as_str(),
                    "trace" | "debug" | "info" | "warn" | "error"
                ) {
                    (lower, 1usize)
                } else {
                    ("info".to_string(), 0usize)
                }
            }
            None => ("info".to_string(), 0usize),
        };

        let parts = values
            .into_iter()
            .skip(start_idx)
            .map(|value| stringify_lua_value(&value).map_err(LuaError::external))
            .collect::<Result<Vec<_>, _>>()?;
        let message = parts.join(" ");

        match level.as_str() {
            "trace" => trace!("<lua> {}", message),
            "debug" => debug!("<lua> {}", message),
            "warn" => warn!("<lua> {}", message),
            "error" => error!("<lua> {}", message),
            _ => info!("<lua> {}", message),
        }

        Ok(())
    })?;
    globals.set("log", log_fn)?;

    // uuid4() -> random UUID string
    let uuid_fn = lua.create_function(|_, ()| Ok(Uuid::new_v4().to_string()))?;
    globals.set("uuid4", uuid_fn)?;

    // now_rfc3339() -> RFC3339 timestamp
    let now_fn = lua.create_function(|_, ()| Ok(Utc::now().to_rfc3339()))?;
    globals.set("now_rfc3339", now_fn)?;

    // now_unix_ms() -> epoch milliseconds
    let now_unix_fn = lua.create_function(|_, ()| Ok(Utc::now().timestamp_millis()))?;
    globals.set("now_unix_ms", now_unix_fn)?;

    // ctx table
    let ctx_value = json_value_to_lua_table(
        lua,
        &serde_json::Value::Object(ctx.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
    )?;
    globals.set("ctx", ctx_value.clone())?;

    Ok(ctx_value)
}

fn stringify_lua_value(value: &LuaValue) -> Result<String> {
    match value {
        LuaValue::String(s) => Ok(s.to_str()?.to_string()),
        LuaValue::Boolean(b) => Ok(b.to_string()),
        LuaValue::Integer(i) => Ok(i.to_string()),
        LuaValue::Number(n) => Ok(n.to_string()),
        LuaValue::Nil => Ok("nil".to_string()),
        _ => {
            let json = lua_value_to_json(value)?;
            Ok(match serde_json::to_string(&json) {
                Ok(serialized) => serialized,
                Err(_) => format!("{:?}", value),
            })
        }
    }
}
