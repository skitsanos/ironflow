use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use mlua::prelude::*;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::nodes::builtin::code_node::{json_value_to_lua_table, lua_value_to_json};
use crate::nodes::builtin::lua_sandbox;

pub struct ForEachNode;

#[async_trait]
impl Node for ForEachNode {
    fn node_type(&self) -> &str {
        "foreach"
    }

    fn description(&self) -> &str {
        "Iterate over an array, execute a Lua function per item, and collect results"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("foreach requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("foreach_results");

        let b64 = config
            .get("transform_bytecode_b64")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("foreach requires 'transform' to be a function"))?;

        let filter_nulls = config
            .get("filter_nulls")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let items = source
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))?;

        let lua = Lua::new();
        lua_sandbox::setup_sandbox(&lua, &ctx)?;

        // Decode and load the transform function
        let bytecode = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| anyhow::anyhow!("Failed to decode transform bytecode: {}", e))?;
        let func: LuaFunction = lua
            .load(&bytecode)
            .into_function()
            .map_err(|e| anyhow::anyhow!("Failed to load transform function: {}", e))?;

        let mut results = Vec::with_capacity(items.len());

        for (i, item_json) in items.iter().enumerate() {
            let item_lua = json_value_to_lua_table(&lua, item_json)?;

            let result: LuaValue = func.call((item_lua, (i + 1) as i64)).map_err(|e| {
                anyhow::anyhow!(
                    "foreach transform failed on item {} (index {}): {}",
                    i,
                    i + 1,
                    e
                )
            })?;

            let json_val = lua_value_to_json(&result)?;
            if filter_nulls && json_val.is_null() {
                continue;
            }
            results.push(json_val);
        }

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::Value::Array(results.clone()),
        );
        output.insert(
            format!("{}_count", output_key),
            serde_json::json!(results.len()),
        );
        Ok(output)
    }
}
