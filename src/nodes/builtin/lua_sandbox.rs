use anyhow::Result;
use base64::Engine;
use mlua::prelude::*;

use crate::engine::types::Context;
use crate::nodes::builtin::code_node::json_value_to_lua_table;

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

    // ctx table
    let ctx_value = json_value_to_lua_table(
        lua,
        &serde_json::Value::Object(ctx.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
    )?;
    globals.set("ctx", ctx_value.clone())?;

    Ok(ctx_value)
}
