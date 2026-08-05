use anyhow::Result;
use chrono::Utc;
use mlua::prelude::*;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::engine::types::FlowDefinition;
use crate::lua::analysis::{HandlerDiagnostics, LuaDiagnostic};
use crate::lua::conversion::{lua_to_log_string, register_json_globals};
use crate::lua::sandbox::new_sandboxed_lua;
use crate::nodes::NodeRegistry;
use crate::util::execution::{ExecutionControl, run_blocking_step};
use crate::util::limits::{
    LuaExecutionLimits, apply_lua_limits, apply_lua_limits_with_control, collect_lua_garbage,
    max_flow_source_bytes,
};

use super::api::register_flow_api;
use super::extractor::extract_flow;
use super::source;

/// Lua runtime for loading and parsing flow definitions.
pub struct LuaRuntime;

#[derive(Debug)]
pub struct ValidatedFlow {
    pub flow: FlowDefinition,
    pub warnings: Vec<LuaDiagnostic>,
}

impl LuaRuntime {
    /// Load a flow definition from a Lua file.
    pub fn load_flow(path: &str, registry: &NodeRegistry) -> Result<FlowDefinition> {
        Self::load_flow_controlled(path, registry, None, false).map(|loaded| loaded.flow)
    }

    /// Load a flow file off the async runtime worker while observing the
    /// enclosing step deadline and cancellation.
    pub async fn load_flow_async(path: &str, registry: &NodeRegistry) -> Result<FlowDefinition> {
        let path = path.to_owned();
        let registry = registry.snapshot();
        run_blocking_step(move |execution| {
            Self::load_flow_controlled(&path, &registry, Some(execution), false)
                .map(|loaded| loaded.flow)
        })
        .await
    }

    /// Load a flow file and collect non-fatal handler diagnostics.
    pub fn validate_flow(path: &str, registry: &NodeRegistry) -> Result<ValidatedFlow> {
        Self::load_flow_controlled(path, registry, None, true)
    }

    /// Validate a flow file off the async runtime worker.
    pub async fn validate_flow_async(path: &str, registry: &NodeRegistry) -> Result<ValidatedFlow> {
        let path = path.to_owned();
        let registry = registry.snapshot();
        run_blocking_step(move |execution| {
            Self::load_flow_controlled(&path, &registry, Some(execution), true)
        })
        .await
    }

    fn load_flow_controlled(
        path: &str,
        registry: &NodeRegistry,
        execution: Option<ExecutionControl>,
        collect_diagnostics: bool,
    ) -> Result<ValidatedFlow> {
        checkpoint(&execution)?;
        let source = source::read_file(path, max_flow_source_bytes(), execution.as_ref())?;
        checkpoint(&execution)?;
        Self::load_source(&source, path, registry, execution, collect_diagnostics)
    }

    /// Load a flow definition from a Lua string.
    pub fn load_flow_from_string(source: &str, registry: &NodeRegistry) -> Result<FlowDefinition> {
        Self::load_flow_from_string_controlled(source, registry, None, false)
            .map(|loaded| loaded.flow)
    }

    /// Load an inline flow off the async runtime worker while observing the
    /// enclosing step deadline and cancellation.
    pub async fn load_flow_from_string_async(
        source: &str,
        registry: &NodeRegistry,
    ) -> Result<FlowDefinition> {
        source::validate(source, max_flow_source_bytes())?;
        let source = source.to_owned();
        let registry = registry.snapshot();
        run_blocking_step(move |execution| {
            Self::load_flow_from_string_controlled(&source, &registry, Some(execution), false)
                .map(|loaded| loaded.flow)
        })
        .await
    }

    /// Load inline flow source and collect non-fatal handler diagnostics.
    pub fn validate_flow_from_string(
        source: &str,
        registry: &NodeRegistry,
    ) -> Result<ValidatedFlow> {
        Self::load_flow_from_string_controlled(source, registry, None, true)
    }

    /// Validate inline flow source off the async runtime worker.
    pub async fn validate_flow_from_string_async(
        source: &str,
        registry: &NodeRegistry,
    ) -> Result<ValidatedFlow> {
        source::validate(source, max_flow_source_bytes())?;
        let source = source.to_owned();
        let registry = registry.snapshot();
        run_blocking_step(move |execution| {
            Self::load_flow_from_string_controlled(&source, &registry, Some(execution), true)
        })
        .await
    }

    fn load_flow_from_string_controlled(
        source: &str,
        registry: &NodeRegistry,
        execution: Option<ExecutionControl>,
        collect_diagnostics: bool,
    ) -> Result<ValidatedFlow> {
        checkpoint(&execution)?;
        source::validate(source, max_flow_source_bytes())?;
        checkpoint(&execution)?;

        Self::load_source(source, "<inline>", registry, execution, collect_diagnostics)
    }

    fn load_source(
        source: &str,
        source_name: &str,
        registry: &NodeRegistry,
        execution: Option<ExecutionControl>,
        collect_diagnostics: bool,
    ) -> Result<ValidatedFlow> {
        let diagnostics = collect_diagnostics
            .then(|| HandlerDiagnostics::analyze(source))
            .transpose()?;
        let lua = new_sandboxed_lua()?;
        let limits = LuaExecutionLimits::from_env();
        apply_limits(&lua, limits, execution.clone())?;
        Self::setup_sandbox(&lua)?;
        checkpoint(&execution)?;
        register_flow_api(&lua, registry, diagnostics.clone())?;
        checkpoint(&execution)?;

        let flow_table: LuaTable =
            lua.load(source)
                .set_name(source_name)
                .eval()
                .map_err(|error| {
                    if source_name == "<inline>" {
                        anyhow::anyhow!("Failed to evaluate flow source: {error}")
                    } else {
                        anyhow::anyhow!("Failed to evaluate flow file '{source_name}': {error}")
                    }
                })?;
        checkpoint(&execution)?;
        collect_lua_garbage(&lua, limits)?;
        checkpoint(&execution)?;

        let flow = extract_flow(&lua, &flow_table)?;
        checkpoint(&execution)?;
        let warnings = diagnostics
            .map(|analysis| analysis.warnings())
            .transpose()?
            .unwrap_or_default();
        Ok(ValidatedFlow { flow, warnings })
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
