use anyhow::Result;
use mlua::prelude::*;

/// Convert a Lua value to `serde_json::Value`.
pub(super) fn lua_value_to_json(value: &LuaValue) -> Result<serde_json::Value> {
    match value {
        LuaValue::Nil => Ok(serde_json::Value::Null),
        LuaValue::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        LuaValue::Integer(n) => Ok(serde_json::json!(*n)),
        LuaValue::Number(n) => Ok(serde_json::json!(*n)),
        LuaValue::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        LuaValue::Table(t) => lua_table_to_json(t),
        _ => Ok(serde_json::Value::Null),
    }
}

/// Convert a Lua table to `serde_json::Value`.
pub(super) fn lua_table_to_json(table: &LuaTable) -> Result<serde_json::Value> {
    // Check if this is an array (sequential integer keys starting from 1)
    let len = table.len()?;
    let mut is_array = len > 0;

    if is_array {
        // Verify it's a proper sequence
        for i in 1..=len {
            if table.get::<LuaValue>(i).is_err() {
                is_array = false;
                break;
            }
        }
    }

    if is_array {
        let mut arr = Vec::new();
        for i in 1..=len {
            let val: LuaValue = table.get(i)?;
            arr.push(lua_value_to_json(&val)?);
        }
        Ok(serde_json::Value::Array(arr))
    } else {
        let mut map = serde_json::Map::new();
        for pair in table.pairs::<String, LuaValue>() {
            let (key, val) = pair?;
            map.insert(key, lua_value_to_json(&val)?);
        }
        Ok(serde_json::Value::Object(map))
    }
}

/// Coerce a Lua value into a string for log output.
pub(super) fn lua_to_log_string(value: &LuaValue) -> Result<String> {
    match value {
        LuaValue::String(s) => Ok(s.to_str()?.to_string()),
        LuaValue::Boolean(b) => Ok(b.to_string()),
        LuaValue::Integer(i) => Ok(i.to_string()),
        LuaValue::Number(n) => Ok(n.to_string()),
        LuaValue::Nil => Ok("nil".to_string()),
        _ => match serde_json::to_string(&lua_value_to_json(value)?) {
            Ok(serialized) => Ok(serialized),
            Err(_) => Ok(format!("{:?}", value)),
        },
    }
}
