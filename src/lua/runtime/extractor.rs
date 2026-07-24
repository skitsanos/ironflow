use anyhow::Result;
use mlua::prelude::*;

use crate::engine::types::{FlowDefinition, RetryConfig, StepDefinition};
use crate::lua::conversion::lua_table_to_json_at;

/// Turn the Lua-built flow table into a `FlowDefinition`.
pub(super) fn extract_flow(lua: &Lua, flow_table: &LuaTable) -> Result<FlowDefinition> {
    let name: String = flow_table
        .get("_name")
        .map_err(|e| anyhow::anyhow!("Flow must have a name: {}", e))?;

    let steps_table: LuaTable = flow_table
        .get("_steps")
        .map_err(|e| anyhow::anyhow!("Flow must have steps: {}", e))?;
    let step_count: usize = flow_table
        .get("_step_count")
        .map_err(|e| anyhow::anyhow!("Flow has an invalid step count: {}", e))?;

    let mut steps = Vec::with_capacity(step_count);
    let mut seen_names = std::collections::HashSet::new();

    // `_steps` is an engine-owned numeric sequence. Read it by index instead
    // of using `pairs()`: flow declaration order is part of the execution
    // contract and determines deterministic same-phase output precedence.
    for index in 1..=step_count {
        let step_table: LuaTable = steps_table.get(index)?;

        let step_name: String = step_table.get("name")?;

        if !seen_names.insert(step_name.clone()) {
            anyhow::bail!(
                "Duplicate step name '{}' in flow '{}'. Each step must have a unique name.",
                step_name,
                name
            );
        }
        let node_type: String = step_table.get("node_type")?;
        let max_retries: u32 = step_table.get("max_retries").unwrap_or(0);
        let backoff_s: f64 = step_table.get("backoff_s").unwrap_or(1.0);
        let timeout_s: Option<f64> = step_table.get("timeout_s").ok();
        let route: Option<String> = step_table.get("route").ok();
        let on_error: Option<String> = step_table.get("on_error").ok();

        // Extract dependencies
        let deps_table: LuaTable = step_table.get("dependencies")?;
        let dependency_count = deps_table.len()? as usize;
        let mut dependencies = Vec::with_capacity(dependency_count);
        for index in 1..=dependency_count {
            dependencies.push(deps_table.get(index)?);
        }

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

        steps.push(StepDefinition {
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
        });
    }

    Ok(FlowDefinition { name, steps })
}
