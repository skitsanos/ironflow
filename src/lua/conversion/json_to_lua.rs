use anyhow::{Result, bail};
use mlua::prelude::*;

use crate::engine::types::Context;

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
            serde_json::Value::Object(object) => self.convert_object(object.iter(), path, depth),
        }
    }

    pub(super) fn convert_context(&mut self, ctx: &Context) -> Result<LuaValue> {
        self.visit("$", 0)?;
        // Retain the JSON object's deterministic key order without cloning its values.
        let fields = ctx.iter().collect::<std::collections::BTreeMap<_, _>>();
        self.convert_object(fields.into_iter(), "$", 0)
    }

    fn visit(&mut self, path: &str, depth: usize) -> Result<()> {
        if depth > self.limits.max_depth {
            bail!(
                "JSON-to-Lua maximum depth {} exceeded at {path} (raise IRONFLOW_MAX_CONVERSION_DEPTH)",
                self.limits.max_depth
            );
        }
        if self.nodes >= self.limits.max_nodes {
            bail!(
                "JSON-to-Lua maximum node count {} exceeded at {path}. This counts the whole value being converted, including all selected context values; code nodes and function handlers can exclude unused values with context_keys (raise IRONFLOW_MAX_CONVERSION_NODES to allow more)",
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

    fn convert_object<'a>(
        &mut self,
        object: impl ExactSizeIterator<Item = (&'a String, &'a serde_json::Value)>,
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
