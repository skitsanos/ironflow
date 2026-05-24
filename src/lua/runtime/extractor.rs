use anyhow::Result;
use mlua::prelude::*;

use crate::engine::types::{FlowDefinition, RetryConfig, StepDefinition};

use super::conversion::lua_table_to_json;

/// Turn the Lua-built flow table into a `FlowDefinition`.
pub(super) fn extract_flow(flow_table: &LuaTable) -> Result<FlowDefinition> {
    let name: String = flow_table
        .get("_name")
        .map_err(|e| anyhow::anyhow!("Flow must have a name: {}", e))?;

    let steps_table: LuaTable = flow_table
        .get("_steps")
        .map_err(|e| anyhow::anyhow!("Flow must have steps: {}", e))?;

    let mut steps = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    for pair in steps_table.pairs::<i32, LuaTable>() {
        let (_, step_table) = pair?;

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
        let mut dependencies = Vec::new();
        for dep_pair in deps_table.pairs::<i32, String>() {
            let (_, dep) = dep_pair?;
            dependencies.push(dep);
        }

        // Extract config (the node config table minus internal keys)
        let config_table: LuaTable = step_table.get("config")?;
        let config = lua_table_to_json(&config_table)?;

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
