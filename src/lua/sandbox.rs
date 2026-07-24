use anyhow::Result;
use base64::Engine;
use chrono::Utc;
use mlua::prelude::*;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::engine::types::Context;
use crate::lua::conversion::{json_value_to_lua, lua_to_log_string, register_json_globals};

/// Read an environment variable for the Lua `env()` global, honoring an
/// optional allowlist (IF-052b).
///
/// When `IRONFLOW_ENV_ALLOWLIST` is set (a comma-separated list of variable
/// names), `env()` only exposes those variables and returns `nil` for every
/// other key. When it is unset, any process variable is readable, which is the
/// documented default. An empty allowlist therefore denies every key.
pub(crate) fn env_lookup(key: &str) -> Option<String> {
    if let Ok(raw) = std::env::var("IRONFLOW_ENV_ALLOWLIST") {
        let permitted = raw
            .split(',')
            .map(str::trim)
            .any(|entry| !entry.is_empty() && entry == key);
        if !permitted {
            return None;
        }
    }
    std::env::var(key).ok()
}

/// Create a Lua VM with only computation-oriented standard libraries.
///
/// `Lua::new()` includes the package, OS, and I/O libraries in its "safe"
/// standard-library set. Clearing their globals afterwards is not a sandbox:
/// `require("os")` can recover the already-loaded OS table from
/// `package.loaded`. Start from an allowlist instead so those libraries never
/// enter the VM registry.
pub(crate) fn new_sandboxed_lua() -> Result<Lua> {
    let libraries = LuaStdLib::TABLE | LuaStdLib::STRING | LuaStdLib::UTF8 | LuaStdLib::MATH;

    let lua = Lua::new_with(libraries, LuaOptions::default())?;
    let globals = lua.globals();
    for name in &[
        "os",
        "io",
        "debug",
        "package",
        "require",
        "load",
        "loadfile",
        "dofile",
        "collectgarbage",
    ] {
        globals.set(*name, LuaValue::Nil)?;
    }
    if let Ok(string_library) = globals.get::<LuaTable>("string") {
        string_library.set("dump", LuaValue::Nil)?;
    }

    Ok(lua)
}

/// Set up the sandboxed Lua environment with standard globals.
///
/// - Uses a VM created by [`new_sandboxed_lua`], without package, OS, or I/O
///   libraries
/// - Removes dynamic code/bytecode loading and caller-controlled GC controls
/// - Exposes `env(key)` for reading environment variables
/// - Exposes `base64_encode(str)` and `base64_decode(str)`
/// - Exposes the workflow `ctx` table
///
/// Returns the `ctx` Lua value for callers that need it.
pub(crate) fn setup_sandbox(lua: &Lua, ctx: &Context) -> Result<LuaValue> {
    let globals = lua.globals();

    // env(key) -> string | nil
    let env_fn = lua.create_function(|lua_ctx, key: String| match env_lookup(&key) {
        Some(val) => Ok(LuaValue::String(lua_ctx.create_string(&val)?)),
        None => Ok(LuaValue::Nil),
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

    register_json_globals(lua)?;

    // log([level], message...)
    let log_fn = lua.create_function(|lua_ctx, args: LuaMultiValue| {
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
            .map(|value| lua_to_log_string(lua_ctx, &value).map_err(LuaError::external))
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
    let ctx_value = json_value_to_lua(
        lua,
        &serde_json::Value::Object(ctx.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
    )?;
    globals.set("ctx", ctx_value.clone())?;

    Ok(ctx_value)
}
