use anyhow::{Context as _, Result};
use mlua::prelude::*;

use crate::engine::types::{FlowDefinition, RetryConfig, StepDefinition};
use crate::lua::context_keys::{CONTEXT_KEYS_UNSUPPORTED, parse_context_keys};
use crate::lua::conversion::lua_table_to_json_at;

/// Most steps one flow may declare and most dependencies one step may list.
///
/// Both counts are read from tables the flow script can write, so they are
/// capped before any memory is reserved for them (IF-175).
const MAX_FLOW_STEPS: usize = 10_000;

/// Turn the Lua-built flow table into a `FlowDefinition`.
pub(super) fn extract_flow(lua: &Lua, flow_table: &LuaTable) -> Result<FlowDefinition> {
    let name: String = flow_table
        .get("_name")
        .map_err(|e| anyhow::anyhow!("Flow must have a name: {}", e))?;

    let steps_table: LuaTable = flow_table
        .get("_steps")
        .map_err(|e| anyhow::anyhow!("Flow must have steps: {}", e))?;
    let step_count = sequence_len(&steps_table, "Flow steps")?;
    ensure_declared_step_count(flow_table, step_count)?;
    if step_count > MAX_FLOW_STEPS {
        anyhow::bail!("Flow '{name}' declares {step_count} steps; the limit is {MAX_FLOW_STEPS}");
    }

    let mut steps = Vec::new();
    steps
        .try_reserve_exact(step_count)
        .context("cannot reserve memory for the flow steps")?;
    let mut seen_names = std::collections::HashSet::new();

    // `_steps` is an engine-owned numeric sequence. Read it by index instead
    // of using `pairs()`: flow declaration order is part of the execution
    // contract and determines deterministic same-phase output precedence.
    for index in 1..=step_count {
        let step_table: LuaTable = steps_table.raw_get(index)?;

        let step_name: String = step_table.get("name")?;

        if !seen_names.insert(step_name.clone()) {
            anyhow::bail!(
                "Duplicate step name '{}' in flow '{}'. Each step must have a unique name.",
                step_name,
                name
            );
        }
        steps.push(extract_step(lua, &step_table, step_name)?);
    }

    Ok(FlowDefinition { name, steps })
}

/// Length of an engine-owned Lua sequence, read without running script code.
///
/// `#table` and indexing would call a metatable's `__len` and `__index`, so a
/// sequence that carries a metatable is rejected and the rest are measured raw.
fn sequence_len(table: &LuaTable, subject: &str) -> Result<usize> {
    if table.metatable().is_some() {
        anyhow::bail!("{subject} must be a plain table without a metatable");
    }
    Ok(table.raw_len())
}

/// `_step_count` is written by the builder next to `_steps`. It never sizes
/// anything; a value that disagrees with the steps the flow holds means the
/// table was not produced by `Flow.new` and `flow:step`.
fn ensure_declared_step_count(flow_table: &LuaTable, step_count: usize) -> Result<()> {
    let declared: LuaValue = flow_table
        .get("_step_count")
        .map_err(|e| anyhow::anyhow!("Flow has an invalid step count: {}", e))?;
    match declared {
        LuaValue::Integer(count) if usize::try_from(count) == Ok(step_count) => Ok(()),
        _ => anyhow::bail!(
            "Flow has an invalid step count: `_step_count` does not match the {step_count} step(s) in `_steps`"
        ),
    }
}

fn extract_step(lua: &Lua, step_table: &LuaTable, step_name: String) -> Result<StepDefinition> {
    let node_type: String = step_table.get("node_type")?;
    let max_retries: u32 = step_table.get("max_retries").unwrap_or(0);
    let backoff_s: f64 = step_table.get("backoff_s").unwrap_or(1.0);
    let timeout_s: Option<f64> = step_table.get("timeout_s").ok();
    let route: Option<String> = step_table.get("route").ok();
    let on_error: Option<String> = step_table.get("on_error").ok();

    let dependencies = extract_dependencies(step_table, &step_name)?;

    // Extract config (the node config table minus internal keys)
    let config_table: LuaTable = step_table.get("config")?;
    let config_path = format!("$.steps[{}].config", serde_json::to_string(&step_name)?);
    let config = lua_table_to_json_at(lua, &config_table, &config_path)?;

    // Inject step name into config for conditional nodes
    let config = match config {
        serde_json::Value::Object(mut m) => {
            m.insert(
                "_step_name".to_string(),
                serde_json::Value::String(step_name.clone()),
            );
            m.remove("_node_type");
            serde_json::Value::Object(m)
        }
        other => other,
    };

    // The builder form already rejects non-code steps; the descriptor
    // form (`nodes.log({ context_keys = ... })`) must not pass silently.
    if node_type == "code" {
        parse_context_keys(&config)
            .map_err(|error| anyhow::anyhow!("Step '{step_name}': {error}"))?;
    } else if config.get("context_keys").is_some() {
        anyhow::bail!("Step '{step_name}': {CONTEXT_KEYS_UNSUPPORTED}");
    }

    Ok(StepDefinition {
        name: step_name,
        node_type,
        config,
        dependencies,
        retry: RetryConfig {
            max_retries,
            backoff_s,
        },
        timeout_s,
        route,
        on_error,
    })
}

fn extract_dependencies(step_table: &LuaTable, step_name: &str) -> Result<Vec<String>> {
    let deps_table: LuaTable = step_table.get("dependencies")?;
    let count = sequence_len(&deps_table, &format!("Step '{step_name}' dependencies"))?;
    if count > MAX_FLOW_STEPS {
        anyhow::bail!(
            "Step '{step_name}' declares {count} dependencies; the limit is {MAX_FLOW_STEPS}"
        );
    }
    let mut dependencies = Vec::new();
    dependencies
        .try_reserve_exact(count)
        .with_context(|| format!("cannot reserve memory for step '{step_name}' dependencies"))?;
    for index in 1..=count {
        dependencies.push(deps_table.raw_get(index)?);
    }
    Ok(dependencies)
}
