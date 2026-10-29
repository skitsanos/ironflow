use anyhow::Result;
use mlua::prelude::*;

use crate::engine::types::FlowDefinition;
use crate::lua::analysis::{HandlerDiagnostics, LuaDiagnostic};
use crate::lua::sandbox::{new_sandboxed_lua, register_shared_globals};
use crate::nodes::NodeRegistry;
use crate::util::execution::{ExecutionControl, run_blocking_step};
use crate::util::limits::{
    LuaExecutionLimits, apply_lua_limits, apply_lua_limits_with_control, collect_lua_garbage,
    max_flow_source_bytes,
};

use super::api::register_flow_api;
use super::extractor::extract_flow;
use super::source;
use super::validation::validate_code_sources;

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
        register_shared_globals(&lua)?;
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
        let mut warnings = diagnostics
            .map(|analysis| analysis.warnings())
            .transpose()?
            .unwrap_or_default();
        if collect_diagnostics {
            warnings.extend(validate_code_sources(&lua, &flow)?);
        }
        Ok(ValidatedFlow { flow, warnings })
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
