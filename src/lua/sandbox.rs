use anyhow::Result;
use base64::Engine;
use chrono::Utc;
use mlua::prelude::*;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::engine::types::Context;
use crate::lua::conversion::{context_to_lua, lua_to_log_string, register_json_globals};

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

/// Register the globals shared by flow loading and code-node execution:
/// `env`, the JSON helpers, `log`, `uuid4`, `now_rfc3339` and `now_unix_ms`.
pub(crate) fn register_shared_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();

    // env(key) -> string | nil
    let env_fn = lua.create_function(|lua_ctx, key: String| match env_lookup(&key) {
        Some(val) => Ok(LuaValue::String(lua_ctx.create_string(&val)?)),
        None => Ok(LuaValue::Nil),
    })?;
    globals.set("env", env_fn)?;

    register_json_globals(lua)?;

    // log([level], message...)
    globals.set("log", lua.create_function(log_from_lua)?)?;

    // uuid4() -> random UUID string
    let uuid_fn = lua.create_function(|_, ()| Ok(Uuid::new_v4().to_string()))?;
    globals.set("uuid4", uuid_fn)?;

    // now_rfc3339() -> RFC3339 timestamp
    let now_fn = lua.create_function(|_, ()| Ok(Utc::now().to_rfc3339()))?;
    globals.set("now_rfc3339", now_fn)?;

    // now_unix_ms() -> epoch milliseconds
    let now_unix_fn = lua.create_function(|_, ()| Ok(Utc::now().timestamp_millis()))?;
    globals.set("now_unix_ms", now_unix_fn)?;

    Ok(())
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
    register_shared_globals(lua)?;
    let globals = lua.globals();

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

    // ctx table
    let ctx_value = context_to_lua(lua, ctx)?;
    globals.set("ctx", ctx_value.clone())?;

    Ok(ctx_value)
}

/// `log([level], message...)`: an optional leading level name selects the
/// tracing level; every remaining argument is rendered and joined with spaces.
fn log_from_lua(lua: &Lua, args: LuaMultiValue) -> LuaResult<()> {
    let values = args.into_iter().collect::<Vec<LuaValue>>();
    if values.is_empty() {
        return Err(LuaError::RuntimeError(
            "log() requires at least one argument".into(),
        ));
    }

    let (level, start_idx) = log_level(values.first())?;
    let parts = values
        .into_iter()
        .skip(start_idx)
        .map(|value| lua_to_log_string(lua, &value).map_err(LuaError::external))
        .collect::<Result<Vec<_>, _>>()?;
    emit_log(&level, &parts.join(" "));
    Ok(())
}

/// A recognized level name consumes the first argument; anything else logs at
/// `info` and keeps the first argument as message text.
fn log_level(first: Option<&LuaValue>) -> LuaResult<(String, usize)> {
    match first.and_then(|value| value.as_string()) {
        Some(level) => {
            let lower = level.to_str()?.to_lowercase();
            if matches!(
                lower.as_str(),
                "trace" | "debug" | "info" | "warn" | "error"
            ) {
                Ok((lower, 1))
            } else {
                Ok(("info".to_string(), 0))
            }
        }
        None => Ok(("info".to_string(), 0)),
    }
}

#[allow(clippy::cognitive_complexity)] // Five tracing macros, one per level: a call site needs a constant level.
fn emit_log(level: &str, message: &str) {
    match level {
        "trace" => trace!("<lua> {}", message),
        "debug" => debug!("<lua> {}", message),
        "warn" => warn!("<lua> {}", message),
        "error" => error!("<lua> {}", message),
        _ => info!("<lua> {}", message),
    }
}
