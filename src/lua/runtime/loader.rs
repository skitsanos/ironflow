use anyhow::Result;
use chrono::Utc;
use mlua::prelude::*;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::engine::types::FlowDefinition;
use crate::lua::conversion::{lua_to_log_string, register_json_globals};
use crate::lua::sandbox::new_sandboxed_lua;
use crate::nodes::NodeRegistry;
use crate::util::execution::{ExecutionControl, run_blocking_step};
use crate::util::limits::{
    LuaExecutionLimits, apply_lua_limits, apply_lua_limits_with_control, collect_lua_garbage,
};

use super::api::register_flow_api;
use super::extractor::extract_flow;

/// Lua runtime for loading and parsing flow definitions.
pub struct LuaRuntime;

impl LuaRuntime {
    /// Load a flow definition from a Lua file.
    pub fn load_flow(path: &str, registry: &NodeRegistry) -> Result<FlowDefinition> {
        Self::load_flow_controlled(path, registry, None)
    }

    /// Load a flow file off the async runtime worker while observing the
    /// enclosing step deadline and cancellation.
    pub async fn load_flow_async(path: &str, registry: &NodeRegistry) -> Result<FlowDefinition> {
        let path = path.to_owned();
        let registry = registry.snapshot();
        run_blocking_step(move |execution| {
            Self::load_flow_controlled(&path, &registry, Some(execution))
        })
        .await
    }

    fn load_flow_controlled(
        path: &str,
        registry: &NodeRegistry,
        execution: Option<ExecutionControl>,
    ) -> Result<FlowDefinition> {
        checkpoint(&execution)?;
        let lua = new_sandboxed_lua()?;
        let limits = LuaExecutionLimits::from_env();
        apply_limits(&lua, limits, execution.clone())?;

        // Sandbox: remove dangerous modules
        Self::setup_sandbox(&lua)?;
        checkpoint(&execution)?;

        // Register the Flow class and nodes table
        register_flow_api(&lua, registry)?;
        checkpoint(&execution)?;

        // Load and execute the Lua file
        let source = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read flow file '{}': {}", path, e))?;
        checkpoint(&execution)?;

        let flow_table: LuaTable = lua
            .load(&source)
            .set_name(path)
            .eval()
            .map_err(|e| anyhow::anyhow!("Failed to evaluate flow file '{}': {}", path, e))?;
        checkpoint(&execution)?;
        collect_lua_garbage(&lua, limits)?;
        checkpoint(&execution)?;

        // Extract the flow definition from the returned table
        let flow = extract_flow(&lua, &flow_table)?;
        checkpoint(&execution)?;
        Ok(flow)
    }

    /// Load a flow definition from a Lua string.
    pub fn load_flow_from_string(source: &str, registry: &NodeRegistry) -> Result<FlowDefinition> {
        Self::load_flow_from_string_controlled(source, registry, None)
    }

    /// Load an inline flow off the async runtime worker while observing the
    /// enclosing step deadline and cancellation.
    pub async fn load_flow_from_string_async(
        source: &str,
        registry: &NodeRegistry,
    ) -> Result<FlowDefinition> {
        let source = source.to_owned();
        let registry = registry.snapshot();
        run_blocking_step(move |execution| {
            Self::load_flow_from_string_controlled(&source, &registry, Some(execution))
        })
        .await
    }

    fn load_flow_from_string_controlled(
        source: &str,
        registry: &NodeRegistry,
        execution: Option<ExecutionControl>,
    ) -> Result<FlowDefinition> {
        checkpoint(&execution)?;
        let lua = new_sandboxed_lua()?;
        let limits = LuaExecutionLimits::from_env();
        apply_limits(&lua, limits, execution.clone())?;
        Self::setup_sandbox(&lua)?;
        checkpoint(&execution)?;
        register_flow_api(&lua, registry)?;
        checkpoint(&execution)?;

        let flow_table: LuaTable = lua
            .load(source)
            .set_name("<inline>")
            .eval()
            .map_err(|e| anyhow::anyhow!("Failed to evaluate flow source: {}", e))?;
        checkpoint(&execution)?;
        collect_lua_garbage(&lua, limits)?;
        checkpoint(&execution)?;

        let flow = extract_flow(&lua, &flow_table)?;
        checkpoint(&execution)?;
        Ok(flow)
    }

    fn setup_sandbox(lua: &Lua) -> Result<()> {
        let globals = lua.globals();

        // Expose a safe env(key) function to read environment variables,
        // honoring the optional IRONFLOW_ENV_ALLOWLIST (IF-052b).
        let env_fn = lua.create_function(|lua_ctx, key: String| {
            match crate::lua::sandbox::env_lookup(&key) {
                Some(val) => Ok(LuaValue::String(lua_ctx.create_string(&val)?)),
                None => Ok(LuaValue::Nil),
            }
        })?;
        globals.set("env", env_fn)?;

        register_json_globals(lua)?;

        // log([level], message...)
        let log_fn = lua.create_function(|lua, args: LuaMultiValue| {
            let values = args.into_iter().collect::<Vec<LuaValue>>();
            if values.is_empty() {
                return Err(LuaError::RuntimeError(
                    "log() requires at least one argument".into(),
                ));
            }

            let (level, start_idx) = match values.first().and_then(|v| v.as_string()) {
                Some(level) => {
                    let lower = level.to_str()?.to_lowercase();
                    if matches!(
                        lower.as_str(),
                        "trace" | "debug" | "info" | "warn" | "error"
                    ) {
                        (lower, 1usize)
                    } else {
                        ("info".to_string(), 0usize)
                    }
                }
                None => ("info".to_string(), 0usize),
            };

            let parts = values
                .into_iter()
                .skip(start_idx)
                .map(|value| lua_to_log_string(lua, &value).map_err(LuaError::external))
                .collect::<Result<Vec<_>, _>>()?;
            let message = parts.join(" ");

            match level.as_str() {
                "trace" => trace!("<lua> {}", message),
                "debug" => debug!("<lua> {}", message),
                "warn" => warn!("<lua> {}", message),
                "error" => error!("<lua> {}", message),
                _ => info!("<lua> {}", message),
            }

            Ok(())
        })?;
        globals.set("log", log_fn)?;

        // uuid4() -> random UUID string
        let uuid_fn = lua.create_function(|_, ()| Ok(Uuid::new_v4().to_string()))?;
        globals.set("uuid4", uuid_fn)?;

        // now_rfc3339() -> RFC3339 timestamp
        let now_fn = lua.create_function(|_, ()| Ok(Utc::now().to_rfc3339()))?;
        globals.set("now_rfc3339", now_fn)?;

        // now_unix_ms() -> epoch milliseconds
        let now_unix_fn = lua.create_function(|_, ()| Ok(Utc::now().timestamp_millis()))?;
        globals.set("now_unix_ms", now_unix_fn)?;

        Ok(())
    }
}

fn apply_limits(
    lua: &Lua,
    limits: LuaExecutionLimits,
    execution: Option<ExecutionControl>,
) -> Result<()> {
    if execution.is_some() {
        apply_lua_limits_with_control(lua, limits, execution)
    } else {
        apply_lua_limits(lua, limits)
    }
}

fn checkpoint(execution: &Option<ExecutionControl>) -> Result<()> {
    if let Some(execution) = execution {
        execution.checkpoint()?;
    }
    Ok(())
}
