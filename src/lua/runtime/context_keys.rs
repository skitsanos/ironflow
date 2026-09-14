use mlua::prelude::*;

use crate::lua::context_keys::parse_context_keys;
use crate::lua::conversion::{json_value_to_lua, lua_value_to_json_at};

/// Canonicalize the Lua empty-list spelling `{}` to a JSON array at the API boundary.
pub(super) fn set_context_keys(lua: &Lua, config: &LuaTable, keys: LuaValue) -> LuaResult<()> {
    if !matches!(keys, LuaValue::Table(_)) {
        return Err(LuaError::RuntimeError(
            "context_keys must be an array of strings".into(),
        ));
    }
    let mut value =
        lua_value_to_json_at(lua, &keys, "$.context_keys").map_err(LuaError::external)?;
    if value.as_object().is_some_and(|object| object.is_empty()) {
        value = serde_json::json!([]);
    }
    parse_context_keys(&serde_json::json!({ "context_keys": &value }))
        .map_err(LuaError::external)?;
    config.set(
        "context_keys",
        json_value_to_lua(lua, &value).map_err(LuaError::external)?,
    )
}

pub(super) fn register_context_keys(lua: &Lua, builder: &LuaTable) -> LuaResult<()> {
    let context_keys = lua.create_function(|lua, (builder, keys): (LuaTable, LuaValue)| {
        let step: LuaTable = builder.get("_step")?;
        if step.get::<String>("node_type")? != "code" {
            return Err(LuaError::RuntimeError(
                "context_keys() is only supported for code nodes and function handlers".into(),
            ));
        }
        let config: LuaTable = step.get("config")?;
        // A node descriptor may be reused by other steps; this option is step-local.
        let projected = lua.create_table()?;
        for pair in config.pairs::<LuaValue, LuaValue>() {
            let (key, value) = pair?;
            projected.raw_set(key, value)?;
        }
        set_context_keys(lua, &projected, keys)?;
        step.set("config", projected)?;
        Ok(builder)
    })?;
    builder.set("context_keys", context_keys)
}
