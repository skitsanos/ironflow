use std::collections::HashMap;

use anyhow::{Result, bail};
use mlua::prelude::*;

use super::path::{json_field_path, positive_index, positive_integer_index};
use super::{ConversionLimits, OBJECT_METATABLE_REGISTRY_KEY};

pub(super) struct LuaToJsonConverter {
    limits: ConversionLimits,
    nodes: usize,
    active_tables: HashMap<usize, String>,
    array_metatable: usize,
    object_metatable: Option<usize>,
}

impl LuaToJsonConverter {
    pub(super) fn new(lua: &Lua, limits: ConversionLimits) -> Self {
        let object_metatable = lua
            .named_registry_value::<LuaTable>(OBJECT_METATABLE_REGISTRY_KEY)
            .ok()
            .map(|table| table.to_pointer() as usize);

        Self {
            limits,
            nodes: 0,
            active_tables: HashMap::new(),
            array_metatable: lua.array_metatable().to_pointer() as usize,
            object_metatable,
        }
    }

    pub(super) fn convert(
        &mut self,
        value: &LuaValue,
        path: &str,
        depth: usize,
    ) -> Result<serde_json::Value> {
        self.visit(path, depth)?;

        if value.is_null() {
            return Ok(serde_json::Value::Null);
        }

        match value {
            LuaValue::Nil => Ok(serde_json::Value::Null),
            LuaValue::Boolean(boolean) => Ok(serde_json::Value::Bool(*boolean)),
            LuaValue::Integer(integer) => Ok(serde_json::Value::Number((*integer).into())),
            LuaValue::Number(number) => serde_json::Number::from_f64(*number)
                .map(serde_json::Value::Number)
                .ok_or_else(|| {
                    anyhow::anyhow!("non-finite Lua number at {path} is not valid JSON")
                }),
            LuaValue::String(string) => Ok(serde_json::Value::String(
                string
                    .to_str()
                    .map_err(|error| {
                        anyhow::anyhow!("Lua string at {path} is not valid UTF-8: {error}")
                    })?
                    .to_string(),
            )),
            LuaValue::Table(table) => self.convert_table(table, path, depth),
            unsupported => bail!(
                "unsupported Lua {} value at {path}; expected a JSON-compatible value",
                unsupported.type_name()
            ),
        }
    }

    fn visit(&mut self, path: &str, depth: usize) -> Result<()> {
        if depth > self.limits.max_depth {
            bail!(
                "Lua-to-JSON maximum depth {} exceeded at {path} (raise IRONFLOW_MAX_CONVERSION_DEPTH)",
                self.limits.max_depth
            );
        }
        if self.nodes >= self.limits.max_nodes {
            bail!(
                "Lua-to-JSON maximum node count {} exceeded at {path} (raise IRONFLOW_MAX_CONVERSION_NODES)",
                self.limits.max_nodes
            );
        }
        self.nodes += 1;
        Ok(())
    }

    fn convert_table(
        &mut self,
        table: &LuaTable,
        path: &str,
        depth: usize,
    ) -> Result<serde_json::Value> {
        let identity = table.to_pointer() as usize;
        if let Some(active_path) = self.active_tables.get(&identity) {
            bail!("cyclic Lua table at {path}; the same table is already active at {active_path}");
        }

        self.active_tables.insert(identity, path.to_string());
        let result = self.convert_table_inner(table, path, depth);
        self.active_tables.remove(&identity);
        result
    }

    fn convert_table_inner(
        &mut self,
        table: &LuaTable,
        path: &str,
        depth: usize,
    ) -> Result<serde_json::Value> {
        let marker = self.table_marker(table);
        let (array_entries, mut object_entries) = self.collect_entries(table, path)?;

        match marker {
            TableMarker::Array if !object_entries.is_empty() => bail!(
                "table marked as a JSON array at {path} contains object field {}",
                object_entries[0].0
            ),
            TableMarker::Object if !array_entries.is_empty() => bail!(
                "table marked as a JSON object at {path} contains array entry {}",
                array_entries[0].1
            ),
            TableMarker::Auto if !array_entries.is_empty() && !object_entries.is_empty() => bail!(
                "mixed Lua table at {path} contains both array entry {} and object field {}; use json_array(...) or json_object(...) with a uniform table",
                array_entries[0].1,
                object_entries[0].0
            ),
            _ => {}
        }

        if marker == TableMarker::Array || !array_entries.is_empty() {
            return self.convert_array_entries(array_entries, path, depth);
        }

        object_entries.sort_unstable_by(|left, right| left.1.cmp(&right.1));
        let mut object = serde_json::Map::new();
        for (entry_path, key, value) in object_entries {
            object.insert(key, self.convert(&value, &entry_path, depth + 1)?);
        }
        Ok(serde_json::Value::Object(object))
    }

    fn collect_entries(
        &self,
        table: &LuaTable,
        path: &str,
    ) -> Result<(Vec<ArrayEntry>, Vec<ObjectEntry>)> {
        let mut array_entries = Vec::new();
        let mut object_entries = Vec::new();

        for pair in table.pairs::<LuaValue, LuaValue>() {
            self.ensure_entry_capacity(array_entries.len() + object_entries.len(), path)?;
            let (key, value) = pair.map_err(|error| {
                anyhow::anyhow!("failed to inspect Lua table at {path}: {error}")
            })?;

            match key {
                LuaValue::String(key) => {
                    let key = key
                        .to_str()
                        .map_err(|error| {
                            anyhow::anyhow!(
                                "object key in Lua table at {path} is not UTF-8: {error}"
                            )
                        })?
                        .to_string();
                    object_entries.push((json_field_path(path, &key), key, value));
                }
                LuaValue::Integer(index) => {
                    let index = positive_integer_index(index, path)?;
                    array_entries.push((index, format!("{path}[{index}]"), value));
                }
                LuaValue::Number(index) => {
                    let index = positive_index(index, path)?;
                    array_entries.push((index, format!("{path}[{index}]"), value));
                }
                unsupported => bail!(
                    "unsupported Lua table key type {} at {path}; JSON keys must be strings or positive array indices",
                    unsupported.type_name()
                ),
            }
        }
        Ok((array_entries, object_entries))
    }

    fn convert_array_entries(
        &mut self,
        mut entries: Vec<ArrayEntry>,
        path: &str,
        depth: usize,
    ) -> Result<serde_json::Value> {
        entries.sort_unstable_by_key(|entry| entry.0);
        let mut array = Vec::with_capacity(entries.len());
        for (offset, (index, entry_path, value)) in entries.into_iter().enumerate() {
            let expected = offset + 1;
            if index != expected {
                bail!(
                    "sparse Lua array at {path}: expected index {expected}, found {index} at {entry_path}"
                );
            }
            array.push(self.convert(&value, &entry_path, depth + 1)?);
        }
        Ok(serde_json::Value::Array(array))
    }

    fn ensure_entry_capacity(&self, collected: usize, path: &str) -> Result<()> {
        let remaining = self.limits.max_nodes.saturating_sub(self.nodes);
        if collected >= remaining {
            bail!(
                "Lua-to-JSON maximum node count {} exceeded while reading table at {path} (raise IRONFLOW_MAX_CONVERSION_NODES)",
                self.limits.max_nodes
            );
        }
        Ok(())
    }

    fn table_marker(&self, table: &LuaTable) -> TableMarker {
        let Some(metatable) = table.metatable() else {
            return TableMarker::Auto;
        };
        let identity = metatable.to_pointer() as usize;
        if identity == self.array_metatable {
            TableMarker::Array
        } else if Some(identity) == self.object_metatable {
            TableMarker::Object
        } else {
            TableMarker::Auto
        }
    }
}

type ArrayEntry = (usize, String, LuaValue);
type ObjectEntry = (String, String, LuaValue);

#[derive(Clone, Copy, PartialEq, Eq)]
enum TableMarker {
    Auto,
    Array,
    Object,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_budget_reports_nested_path() {
        let lua = Lua::new();
        let leaf = lua.create_table().unwrap();
        leaf.set("value", 1).unwrap();
        let middle = lua.create_table().unwrap();
        middle.set("child", leaf).unwrap();
        let root = lua.create_table().unwrap();
        root.set("child", middle).unwrap();

        let limits = ConversionLimits {
            max_depth: 1,
            max_nodes: 100,
        };
        let error = LuaToJsonConverter::new(&lua, limits)
            .convert(&LuaValue::Table(root), "$", 0)
            .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("maximum depth 1"), "{message}");
        assert!(message.contains("$.child.child"), "{message}");
    }

    #[test]
    fn node_budget_rejects_large_table_before_collecting_it() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.set("first", 1).unwrap();
        table.set("second", 2).unwrap();

        let limits = ConversionLimits {
            max_depth: 10,
            max_nodes: 2,
        };
        let error = LuaToJsonConverter::new(&lua, limits)
            .convert(&LuaValue::Table(table), "$", 0)
            .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("maximum node count 2"), "{message}");
        assert!(message.contains("table at $"), "{message}");
    }
}
