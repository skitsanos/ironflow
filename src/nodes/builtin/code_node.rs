use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use mlua::prelude::*;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct CodeNode;

#[async_trait]
impl Node for CodeNode {
    fn node_type(&self) -> &str {
        "code"
    }

    fn description(&self) -> &str {
        "Execute inline Lua code with access to the workflow context"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let lua = Lua::new();

        // Sandbox: remove dangerous modules
        let globals = lua.globals();
        for name in &["os", "io", "debug", "loadfile", "dofile"] {
            globals.set(*name, LuaValue::Nil)?;
        }

        // Expose env() for reading environment variables
        let env_fn = lua.create_function(|lua_ctx, key: String| match std::env::var(&key) {
            Ok(val) => Ok(LuaValue::String(lua_ctx.create_string(&val)?)),
            Err(_) => Ok(LuaValue::Nil),
        })?;
        globals.set("env", env_fn)?;

        // Expose the context as a read-only `ctx` table
        let ctx_table = json_value_to_lua_table(
            &lua,
            &serde_json::Value::Object(ctx.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        )?;
        globals.set("ctx", ctx_table.clone())?;

        // Execute either bytecode (function handler) or source string
        let result: LuaValue =
            if let Some(b64) = config.get("bytecode_b64").and_then(|v| v.as_str()) {
                // Function handler mode: decode bytecode, load, call with ctx
                let bytecode = base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .map_err(|e| anyhow::anyhow!("Failed to decode function bytecode: {}", e))?;
                let func: LuaFunction = lua
                    .load(&bytecode)
                    .into_function()
                    .map_err(|e| anyhow::anyhow!("Failed to load function: {}", e))?;
                func.call(ctx_table)
                    .map_err(|e| anyhow::anyhow!("Function execution failed: {}", e))?
            } else {
                let source = config
                    .get("source")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        anyhow::anyhow!("code node requires 'source' or a function handler")
                    })?;
                lua.load(source)
                    .set_name("<code>")
                    .eval()
                    .map_err(|e| anyhow::anyhow!("Code execution failed: {}", e))?
            };

        // Convert the returned value to NodeOutput
        let mut output = NodeOutput::new();

        match result {
            LuaValue::Table(tbl) => {
                for pair in tbl.pairs::<String, LuaValue>() {
                    let (key, val) = pair?;
                    output.insert(key, lua_value_to_json(&val)?);
                }
            }
            LuaValue::Nil => {
                // No return value — that's fine, nothing to merge
            }
            other => {
                // Single return value — store under "result"
                output.insert("result".to_string(), lua_value_to_json(&other)?);
            }
        }

        Ok(output)
    }
}

/// Convert a serde_json::Value into a Lua value.
fn json_value_to_lua_table(lua: &Lua, value: &serde_json::Value) -> Result<LuaValue> {
    match value {
        serde_json::Value::Null => Ok(LuaValue::Nil),
        serde_json::Value::Bool(b) => Ok(LuaValue::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(LuaValue::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(LuaValue::Number(f))
            } else {
                Ok(LuaValue::Nil)
            }
        }
        serde_json::Value::String(s) => Ok(LuaValue::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let tbl = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                tbl.set(i + 1, json_value_to_lua_table(lua, v)?)?;
            }
            Ok(LuaValue::Table(tbl))
        }
        serde_json::Value::Object(map) => {
            let tbl = lua.create_table()?;
            for (k, v) in map {
                tbl.set(k.as_str(), json_value_to_lua_table(lua, v)?)?;
            }
            Ok(LuaValue::Table(tbl))
        }
    }
}

/// Convert a Lua value back to serde_json::Value.
fn lua_value_to_json(value: &LuaValue) -> Result<serde_json::Value> {
    match value {
        LuaValue::Nil => Ok(serde_json::Value::Null),
        LuaValue::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        LuaValue::Integer(n) => Ok(serde_json::json!(*n)),
        LuaValue::Number(n) => Ok(serde_json::json!(*n)),
        LuaValue::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        LuaValue::Table(t) => {
            // Check if array (sequential integer keys starting from 1)
            let len = t.len()?;
            if len > 0 {
                let mut arr = Vec::new();
                for i in 1..=len {
                    let val: LuaValue = t.get(i)?;
                    arr.push(lua_value_to_json(&val)?);
                }
                Ok(serde_json::Value::Array(arr))
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.pairs::<String, LuaValue>() {
                    let (key, val) = pair?;
                    map.insert(key, lua_value_to_json(&val)?);
                }
                Ok(serde_json::Value::Object(map))
            }
        }
        _ => Ok(serde_json::Value::Null),
    }
}
