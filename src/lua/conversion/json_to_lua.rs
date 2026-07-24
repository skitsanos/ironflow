use anyhow::{Result, bail};
use mlua::prelude::*;

use super::ConversionLimits;
use super::path::json_field_path;

pub(super) struct JsonToLuaConverter<'lua> {
    lua: &'lua Lua,
    limits: ConversionLimits,
    nodes: usize,
}

impl<'lua> JsonToLuaConverter<'lua> {
    pub(super) fn new(lua: &'lua Lua, limits: ConversionLimits) -> Self {
        Self {
            lua,
            limits,
            nodes: 0,
        }
    }

    pub(super) fn convert(
        &mut self,
        value: &serde_json::Value,
        path: &str,
        depth: usize,
    ) -> Result<LuaValue> {
        self.visit(path, depth)?;

        match value {
            serde_json::Value::Null => Ok(self.lua.null()),
            serde_json::Value::Bool(boolean) => Ok(LuaValue::Boolean(*boolean)),
            serde_json::Value::Number(number) => {
                if let Some(integer) = number.as_i64() {
                    Ok(LuaValue::Integer(integer))
                } else if let Some(number) = number.as_f64() {
                    Ok(LuaValue::Number(number))
                } else {
                    bail!("JSON number at {path} cannot be represented in Lua")
                }
            }
            serde_json::Value::String(string) => {
                Ok(LuaValue::String(self.lua.create_string(string)?))
            }
            serde_json::Value::Array(array) => self.convert_array(array, path, depth),
            serde_json::Value::Object(object) => self.convert_object(object, path, depth),
        }
    }

    fn visit(&mut self, path: &str, depth: usize) -> Result<()> {
        if depth > self.limits.max_depth {
            bail!(
                "JSON-to-Lua maximum depth {} exceeded at {path}",
                self.limits.max_depth
            );
        }
        if self.nodes >= self.limits.max_nodes {
            bail!(
                "JSON-to-Lua maximum node count {} exceeded at {path}",
                self.limits.max_nodes
            );
        }
        self.nodes += 1;
        Ok(())
    }

    fn convert_array(
        &mut self,
        array: &[serde_json::Value],
        path: &str,
        depth: usize,
    ) -> Result<LuaValue> {
        let table = self.lua.create_table_with_capacity(array.len(), 0)?;
        for (index, child) in array.iter().enumerate() {
            let child_path = format!("{path}[{index}]");
            table.raw_set(index + 1, self.convert(child, &child_path, depth + 1)?)?;
        }
        table.set_metatable(Some(self.lua.array_metatable()))?;
        Ok(LuaValue::Table(table))
    }

    fn convert_object(
        &mut self,
        object: &serde_json::Map<String, serde_json::Value>,
        path: &str,
        depth: usize,
    ) -> Result<LuaValue> {
        let table = self.lua.create_table_with_capacity(0, object.len())?;
        for (key, child) in object {
            let child_path = json_field_path(path, key);
            table.raw_set(key.as_str(), self.convert(child, &child_path, depth + 1)?)?;
        }
        Ok(LuaValue::Table(table))
    }
}
