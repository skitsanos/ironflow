use anyhow::Result;
use async_trait::async_trait;
use mlua::prelude::*;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::conversion::{json_value_to_lua, lua_value_to_json_at};
use crate::lua::sandbox;
use crate::nodes::Node;
use crate::util::execution::{ExecutionControl, run_blocking_step};
use crate::util::limits::{LuaExecutionLimits, apply_lua_limits_with_control, collect_lua_garbage};
use crate::util::node_config::config_bool;

pub struct ForEachNode;

#[async_trait]
impl Node for ForEachNode {
    fn node_type(&self) -> &str {
        "foreach"
    }

    fn description(&self) -> &str {
        "Iterate over an array, execute a Lua function per item, and collect results"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let config = config.clone();
        let ctx = ctx.clone();
        run_blocking_step(move |execution| execute_foreach(&config, &ctx, execution)).await
    }
}

fn execute_foreach(
    config: &serde_json::Value,
    ctx: &Context,
    execution: ExecutionControl,
) -> Result<NodeOutput> {
    execution.checkpoint()?;
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

    let filter_nulls = config_bool(config, "filter_nulls", ctx).unwrap_or(true);

    let source = ctx
        .get(source_key)
        .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

    let items = source
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))?;

    let lua = sandbox::new_sandboxed_lua()?;
    let limits = LuaExecutionLimits::from_env();
    apply_lua_limits_with_control(&lua, limits, Some(execution.clone()))?;
    sandbox::setup_sandbox(&lua, ctx)?;
    execution.checkpoint()?;

    // Authenticate and load the transform function. Only bytecode this process
    // compiled verifies (IF-035).
    let bytecode = crate::lua::bytecode::verify(b64)?;
    let func: LuaFunction = lua
        .load(&bytecode)
        .into_function()
        .map_err(|e| anyhow::anyhow!("Failed to load transform function: {}", e))?;

    let mut results = Vec::with_capacity(items.len());

    for (i, item_json) in items.iter().enumerate() {
        execution.checkpoint()?;
        let item_lua = json_value_to_lua(&lua, item_json)?;

        let result: LuaValue = func.call((item_lua, (i + 1) as i64)).map_err(|e| {
            anyhow::anyhow!(
                "foreach transform failed on item {} (index {}): {}",
                i,
                i + 1,
                e
            )
        })?;

        let result_path = format!("$foreach[{i}]");
        let json_val = lua_value_to_json_at(&lua, &result, &result_path)?;
        execution.checkpoint()?;
        if filter_nulls && json_val.is_null() {
            continue;
        }
        results.push(json_val);
    }

    let result_count = results.len();
    let mut output = NodeOutput::new();
    output.insert(output_key.to_string(), serde_json::Value::Array(results));
    output.insert(
        format!("{}_count", output_key),
        serde_json::json!(result_count),
    );
    collect_lua_garbage(&lua, limits)?;
    execution.checkpoint()?;
    Ok(output)
}
