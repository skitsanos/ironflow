use anyhow::Result;
use mlua::prelude::*;

use self::json_to_lua::JsonToLuaConverter;
use self::lua_to_json::LuaToJsonConverter;

mod json_to_lua;
mod lua_to_json;
mod path;

pub(super) const OBJECT_METATABLE_REGISTRY_KEY: &str = "__ironflow_json_object_metatable";

#[derive(Clone, Copy)]
pub(super) struct ConversionLimits {
    pub(super) max_depth: usize,
    pub(super) max_nodes: usize,
}

impl Default for ConversionLimits {
    /// Read from the environment on every construction, like every other
    /// `IRONFLOW_MAX_*` ceiling. These were previously hardcoded, which left a
    /// flow that legitimately builds a large structure with no escape hatch
    /// short of recompiling (IF-058).
    fn default() -> Self {
        Self {
            max_depth: crate::util::limits::max_conversion_depth() as usize,
            max_nodes: crate::util::limits::max_conversion_nodes() as usize,
        }
    }
}

/// Register the JSON helpers shared by flow loading and executable Lua nodes.
pub(crate) fn register_json_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let object_metatable = match lua.named_registry_value::<LuaTable>(OBJECT_METATABLE_REGISTRY_KEY)
    {
        Ok(metatable) => metatable,
        Err(_) => {
            let metatable = lua.create_table()?;
            lua.set_named_registry_value(OBJECT_METATABLE_REGISTRY_KEY, metatable.clone())?;
            metatable
        }
    };

    let parse_fn = lua.create_function(|lua, data: String| {
        let json: serde_json::Value = serde_json::from_str(&data)
            .map_err(|error| LuaError::RuntimeError(format!("json_parse failed: {error}")))?;
        json_value_to_lua(lua, &json).map_err(LuaError::external)
    })?;
    globals.set("json_parse", parse_fn)?;

    let stringify_fn = lua.create_function(|lua, value: LuaValue| {
        let json = lua_value_to_json(lua, &value).map_err(LuaError::external)?;
        serde_json::to_string(&json).map_err(|error| LuaError::RuntimeError(error.to_string()))
    })?;
    globals.set("json_stringify", stringify_fn)?;

    let array_fn = lua.create_function(|lua, table: LuaTable| {
        table.set_metatable(Some(lua.array_metatable()))?;
        Ok(table)
    })?;
    globals.set("json_array", array_fn)?;

    let object_fn = lua.create_function(move |_, table: LuaTable| {
        table.set_metatable(Some(object_metatable.clone()))?;
        Ok(table)
    })?;
    globals.set("json_object", object_fn)?;
    globals.set("json_null", lua.null())?;

    Ok(())
}

/// Convert a JSON value to a bounded Lua representation.
///
/// Arrays carry mlua's array metatable so an empty array remains distinct from
/// an empty object. JSON null uses mlua's non-nil null sentinel so object fields
/// and array positions survive a round trip.
pub(crate) fn json_value_to_lua(lua: &Lua, value: &serde_json::Value) -> Result<LuaValue> {
    JsonToLuaConverter::new(lua, ConversionLimits::default()).convert(value, "$", 0)
}

/// Convert a Lua value to JSON using the default bounded conversion policy.
pub(crate) fn lua_value_to_json(lua: &Lua, value: &LuaValue) -> Result<serde_json::Value> {
    lua_value_to_json_at(lua, value, "$")
}

/// Convert a Lua value to JSON and use `path` as the root in diagnostics.
pub(crate) fn lua_value_to_json_at(
    lua: &Lua,
    value: &LuaValue,
    path: &str,
) -> Result<serde_json::Value> {
    LuaToJsonConverter::new(lua, ConversionLimits::default()).convert(value, path, 0)
}

/// Convert a Lua table to JSON and use `path` as the root in diagnostics.
pub(crate) fn lua_table_to_json_at(
    lua: &Lua,
    table: &LuaTable,
    path: &str,
) -> Result<serde_json::Value> {
    lua_value_to_json_at(lua, &LuaValue::Table(table.clone()), path)
}

/// Coerce a Lua value into a string for log output.
pub(crate) fn lua_to_log_string(lua: &Lua, value: &LuaValue) -> Result<String> {
    match value {
        LuaValue::String(string) => Ok(string
            .to_str()
            .map_err(|error| anyhow::anyhow!("Lua log string is not valid UTF-8: {error}"))?
            .to_string()),
        LuaValue::Boolean(boolean) => Ok(boolean.to_string()),
        LuaValue::Integer(integer) => Ok(integer.to_string()),
        LuaValue::Number(number) => Ok(number.to_string()),
        LuaValue::Nil => Ok("nil".to_string()),
        value if value.is_null() => Ok("null".to_string()),
        _ => Ok(serde_json::to_string(&lua_value_to_json(lua, value)?)?),
    }
}
