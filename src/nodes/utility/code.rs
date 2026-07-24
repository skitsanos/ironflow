use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use mlua::prelude::*;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::conversion::lua_value_to_json;
use crate::lua::sandbox;
use crate::nodes::Node;
use crate::util::execution::{ExecutionControl, run_blocking_step};
use crate::util::limits::{LuaExecutionLimits, apply_lua_limits_with_control, collect_lua_garbage};

pub struct CodeNode;

#[async_trait]
impl Node for CodeNode {
    fn node_type(&self) -> &str {
        "code"
    }

    fn description(&self) -> &str {
        "Execute inline Lua code with access to the workflow context"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let config = config.clone();
        let ctx = ctx.clone();
        run_blocking_step(move |execution| execute_code(&config, &ctx, execution)).await
    }
}

fn execute_code(
    config: &serde_json::Value,
    ctx: &Context,
    execution: ExecutionControl,
) -> Result<NodeOutput> {
    execution.checkpoint()?;
    let lua = sandbox::new_sandboxed_lua()?;
    let limits = LuaExecutionLimits::from_env();
    apply_lua_limits_with_control(&lua, limits, Some(execution.clone()))?;
    let ctx_table = sandbox::setup_sandbox(&lua, ctx)?;
    execution.checkpoint()?;

    // Execute either bytecode (function handler) or source string
    let result: LuaValue = if let Some(b64) = config.get("bytecode_b64").and_then(|v| v.as_str()) {
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
            .ok_or_else(|| anyhow::anyhow!("code node requires 'source' or a function handler"))?;
        lua.load(source)
            .set_name("<code>")
            .eval()
            .map_err(|e| anyhow::anyhow!("Code execution failed: {}", e))?
    };

    // Convert the complete result in one pass so table shape, cycles, and
    // conversion budgets are enforced consistently.
    let mut output = NodeOutput::new();
    if !matches!(result, LuaValue::Nil) {
        let result_json = lua_value_to_json(&lua, &result)?;
        execution.checkpoint()?;
        match result_json {
            serde_json::Value::Object(object) => output.extend(object),
            value => {
                output.insert("result".to_string(), value);
            }
        }
    }

    collect_lua_garbage(&lua, limits)?;
    execution.checkpoint()?;

    Ok(output)
}
