//! `json_parse`: JSON text to Lua values in one pass (IF-189).
//!
//! The text is deserialized straight into Lua tables, so every allocation
//! that grows with the document is charged to the Lua memory limit, and the
//! conversion budgets stop construction at the first value over them. No
//! `serde_json::Value` tree is built beside the Lua state.
//!
//! Once a budget is exceeded the rest of the text is still read, without
//! building anything, so that the outcome keeps the order callers rely on:
//! a syntax error first, then the shared storage nesting limit (IF-181),
//! then the conversion budget.

use std::fmt;

use mlua::prelude::*;
use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};

use crate::util::json_depth::{DepthExceeded, MAX_JSON_VALUE_DEPTH};

use super::ConversionLimits;
use super::path::json_field_path;

/// Parse `text` into a Lua value under `limits`.
pub(super) fn parse(lua: &Lua, text: &str, limits: ConversionLimits) -> LuaResult<LuaValue> {
    let mut parser = Parser {
        lua,
        limits,
        nodes: 0,
        path: Vec::new(),
        too_deep_to_store: false,
        refusal: None,
    };
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let parsed = ValueSeed {
        parser: &mut parser,
        depth: 0,
    }
    .deserialize(&mut deserializer)
    .and_then(|value| deserializer.end().map(|()| value));

    let value = parsed.map_err(failed)?;
    if parser.too_deep_to_store {
        return Err(failed(DepthExceeded::value()));
    }
    match parser.refusal {
        Some(refusal) => Err(LuaError::external(refusal)),
        None => Ok(value),
    }
}

fn failed(error: impl fmt::Display) -> LuaError {
    LuaError::RuntimeError(format!("json_parse failed: {error}"))
}

/// Where the value being read sits in its parent.
enum Segment {
    Index(usize),
    Key(LuaString),
}

struct Parser<'lua> {
    lua: &'lua Lua,
    limits: ConversionLimits,
    nodes: usize,
    /// Position of the value being read; rendered only for an error.
    path: Vec<Segment>,
    too_deep_to_store: bool,
    /// The first budget or Lua allocation failure. Nothing is built after it.
    refusal: Option<anyhow::Error>,
}

impl Parser<'_> {
    fn building(&self) -> bool {
        self.refusal.is_none()
    }

    /// Charge one value at `depth` to the conversion budgets. `false` when
    /// the value must not be built.
    fn admit(&mut self, depth: usize) -> bool {
        if !self.building() {
            return false;
        }
        if depth > self.limits.max_depth {
            self.refusal = Some(anyhow::anyhow!(
                "JSON-to-Lua maximum depth {} exceeded at {} (raise IRONFLOW_MAX_CONVERSION_DEPTH)",
                self.limits.max_depth,
                self.rendered_path()
            ));
            return false;
        }
        if self.nodes >= self.limits.max_nodes {
            self.refusal = Some(anyhow::anyhow!(
                "JSON-to-Lua maximum node count {} exceeded at {} (raise IRONFLOW_MAX_CONVERSION_NODES to allow more)",
                self.limits.max_nodes,
                self.rendered_path()
            ));
            return false;
        }
        self.nodes += 1;
        true
    }

    /// Charge one array or object at `depth`. A container nested deeper than
    /// the stores can read back is remembered, whatever else is refused.
    fn admit_container(&mut self, depth: usize) -> bool {
        if depth >= MAX_JSON_VALUE_DEPTH {
            self.too_deep_to_store = true;
        }
        self.admit(depth)
    }

    /// Run a Lua allocation. Its failure, such as the memory limit, ends
    /// construction like a budget does.
    fn lua<T>(&mut self, build: impl FnOnce(&Lua) -> LuaResult<T>) -> Option<T> {
        if !self.building() {
            return None;
        }
        match build(self.lua) {
            Ok(value) => Some(value),
            Err(error) => {
                self.refusal = Some(error.into());
                None
            }
        }
    }

    fn rendered_path(&self) -> String {
        self.path
            .iter()
            .fold("$".to_string(), |parent, segment| match segment {
                Segment::Index(index) => format!("{parent}[{index}]"),
                Segment::Key(key) => json_field_path(&parent, &key.to_string_lossy()),
            })
    }
}

struct ValueSeed<'parser, 'lua> {
    parser: &'parser mut Parser<'lua>,
    depth: usize,
}

impl ValueSeed<'_, '_> {
    fn scalar(self, value: LuaValue) -> LuaValue {
        if self.parser.admit(self.depth) {
            value
        } else {
            LuaValue::Nil
        }
    }
}

impl<'de> DeserializeSeed<'de> for ValueSeed<'_, '_> {
    type Value = LuaValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<LuaValue, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueSeed<'_, '_> {
    type Value = LuaValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_unit<E>(self) -> Result<LuaValue, E> {
        let null = self.parser.lua.null();
        Ok(self.scalar(null))
    }

    fn visit_bool<E>(self, value: bool) -> Result<LuaValue, E> {
        Ok(self.scalar(LuaValue::Boolean(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<LuaValue, E> {
        Ok(self.scalar(LuaValue::Integer(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<LuaValue, E> {
        // Above the Lua integer range a number becomes a float, as before.
        Ok(self.scalar(match i64::try_from(value) {
            Ok(integer) => LuaValue::Integer(integer),
            Err(_) => LuaValue::Number(value as f64),
        }))
    }

    fn visit_f64<E>(self, value: f64) -> Result<LuaValue, E> {
        Ok(self.scalar(LuaValue::Number(value)))
    }

    fn visit_str<E>(self, value: &str) -> Result<LuaValue, E> {
        if !self.parser.admit(self.depth) {
            return Ok(LuaValue::Nil);
        }
        let string = self.parser.lua(|lua| lua.create_string(value));
        Ok(string.map_or(LuaValue::Nil, LuaValue::String))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<LuaValue, A::Error> {
        let Self { parser, depth } = self;
        let table = if parser.admit_container(depth) {
            parser.lua(|lua| lua.create_table())
        } else {
            None
        };
        let mut index = 0;
        loop {
            parser.path.push(Segment::Index(index));
            let element = seq.next_element_seed(ValueSeed {
                parser: &mut *parser,
                depth: depth + 1,
            })?;
            parser.path.pop();
            let Some(element) = element else { break };
            index += 1;
            if let Some(table) = &table {
                parser.lua(|_| table.raw_set(index, element));
            }
        }
        let table = table.and_then(|table| {
            parser.lua(|lua| {
                table.set_metatable(Some(lua.array_metatable()))?;
                Ok(table)
            })
        });
        Ok(table.map_or(LuaValue::Nil, LuaValue::Table))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<LuaValue, A::Error> {
        let Self { parser, depth } = self;
        let table = if parser.admit_container(depth) {
            parser.lua(|lua| lua.create_table())
        } else {
            None
        };
        while let Some(key) = map.next_key_seed(KeySeed {
            parser: &mut *parser,
        })? {
            // The key waits on the path while its value is read.
            let keyed = key.is_some();
            parser.path.extend(key.map(Segment::Key));
            let value = map.next_value_seed(ValueSeed {
                parser: &mut *parser,
                depth: depth + 1,
            })?;
            let key = match keyed.then(|| parser.path.pop()).flatten() {
                Some(Segment::Key(key)) => Some(key),
                _ => None,
            };
            if let (Some(table), Some(key)) = (&table, key) {
                parser.lua(|_| table.raw_set(key, value));
            }
        }
        Ok(table
            .filter(|_| parser.building())
            .map_or(LuaValue::Nil, LuaValue::Table))
    }
}

/// An object key as a Lua string; `None` once nothing is being built.
struct KeySeed<'parser, 'lua> {
    parser: &'parser mut Parser<'lua>,
}

impl<'de> DeserializeSeed<'de> for KeySeed<'_, '_> {
    type Value = Option<LuaString>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for KeySeed<'_, '_> {
    type Value = Option<LuaString>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object key")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(self.parser.lua(|lua| lua.create_string(value)))
    }
}
