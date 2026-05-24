use anyhow::Result;
use chrono::Utc;
use mlua::prelude::*;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::engine::types::FlowDefinition;
use crate::nodes::NodeRegistry;
use crate::nodes::utility::code::json_value_to_lua_table;
use crate::util::limits::{LuaExecutionLimits, apply_lua_limits, collect_lua_garbage};

use super::api::register_flow_api;
use super::conversion::{lua_to_log_string, lua_value_to_json};
use super::extractor::extract_flow;

/// Lua runtime for loading and parsing flow definitions.
pub struct LuaRuntime;

impl LuaRuntime {
    /// Load a flow definition from a Lua file.
    pub fn load_flow(path: &str, registry: &NodeRegistry) -> Result<FlowDefinition> {
        let lua = Lua::new();
        let limits = LuaExecutionLimits::from_env();
        apply_lua_limits(&lua, limits)?;

        // Sandbox: remove dangerous modules
        Self::setup_sandbox(&lua)?;

        // Register the Flow class and nodes table
        register_flow_api(&lua, registry)?;

        // Load and execute the Lua file
        let source = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read flow file '{}': {}", path, e))?;

        let flow_table: LuaTable = lua
            .load(&source)
            .set_name(path)
            .eval()
            .map_err(|e| anyhow::anyhow!("Failed to evaluate flow file '{}': {}", path, e))?;
        collect_lua_garbage(&lua, limits)?;

        // Extract the flow definition from the returned table
        extract_flow(&flow_table)
    }

    /// Load a flow definition from a Lua string.
    pub fn load_flow_from_string(source: &str, registry: &NodeRegistry) -> Result<FlowDefinition> {
        let lua = Lua::new();
        let limits = LuaExecutionLimits::from_env();
        apply_lua_limits(&lua, limits)?;
        Self::setup_sandbox(&lua)?;
        register_flow_api(&lua, registry)?;

        let flow_table: LuaTable = lua
            .load(source)
            .set_name("<inline>")
            .eval()
            .map_err(|e| anyhow::anyhow!("Failed to evaluate flow source: {}", e))?;
        collect_lua_garbage(&lua, limits)?;

        extract_flow(&flow_table)
    }

    fn setup_sandbox(lua: &Lua) -> Result<()> {
        // Remove dangerous globals
        let globals = lua.globals();
        for name in &["os", "io", "debug", "loadfile", "dofile"] {
            globals.set(*name, LuaValue::Nil)?;
        }

        // Expose a safe env(key) function to read environment variables
        let env_fn = lua.create_function(|lua_ctx, key: String| match std::env::var(&key) {
            Ok(val) => Ok(LuaValue::String(lua_ctx.create_string(&val)?)),
            Err(_) => Ok(LuaValue::Nil),
        })?;
        globals.set("env", env_fn)?;

        // json_parse(str) -> Lua table
        let parse_fn = lua.create_function(|lua_ctx, data: String| {
            let json: serde_json::Value = serde_json::from_str(&data)
                .map_err(|e| LuaError::RuntimeError(format!("json_parse failed: {}", e)))?;
            json_value_to_lua_table(lua_ctx, &json).map_err(LuaError::external)
        })?;
        globals.set("json_parse", parse_fn)?;

        // json_stringify(value) -> string
        let stringify_fn = lua.create_function(|_, value: LuaValue| {
            let json_val = lua_value_to_json(&value).map_err(LuaError::external)?;
            let serialized = serde_json::to_string(&json_val)
                .map_err(|e| LuaError::RuntimeError(e.to_string()))?;
            Ok(serialized)
        })?;
        globals.set("json_stringify", stringify_fn)?;

        // log([level], message...)
        let log_fn = lua.create_function(|_, args: LuaMultiValue| {
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
                .map(|value| lua_to_log_string(&value).map_err(LuaError::external))
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
