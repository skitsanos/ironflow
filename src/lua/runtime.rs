use anyhow::Result;
use base64::Engine;
use mlua::prelude::*;

use crate::engine::types::{FlowDefinition, RetryConfig, StepDefinition};
use crate::nodes::NodeRegistry;

/// Lua runtime for loading and parsing flow definitions.
pub struct LuaRuntime;

impl LuaRuntime {
    /// Load a flow definition from a Lua file.
    pub fn load_flow(path: &str, registry: &NodeRegistry) -> Result<FlowDefinition> {
        let lua = Lua::new();

        // Sandbox: remove dangerous modules
        Self::setup_sandbox(&lua)?;

        // Register the Flow class and nodes table
        Self::register_flow_api(&lua, registry)?;

        // Load and execute the Lua file
        let source = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read flow file '{}': {}", path, e))?;

        let flow_table: LuaTable = lua
            .load(&source)
            .set_name(path)
            .eval()
            .map_err(|e| anyhow::anyhow!("Failed to evaluate flow file '{}': {}", path, e))?;

        // Extract the flow definition from the returned table
        Self::extract_flow(&flow_table)
    }

    /// Load a flow definition from a Lua string.
    pub fn load_flow_from_string(source: &str, registry: &NodeRegistry) -> Result<FlowDefinition> {
        let lua = Lua::new();
        Self::setup_sandbox(&lua)?;
        Self::register_flow_api(&lua, registry)?;

        let flow_table: LuaTable = lua
            .load(source)
            .set_name("<inline>")
            .eval()
            .map_err(|e| anyhow::anyhow!("Failed to evaluate flow source: {}", e))?;

        Self::extract_flow(&flow_table)
    }

    fn setup_sandbox(lua: &Lua) -> Result<()> {
        // Remove dangerous globals
        let globals = lua.globals();
        for name in &["os", "io", "debug", "loadfile", "dofile"] {
            globals.set(*name, LuaValue::Nil)?;
        }

        // Expose a safe env(key) function to read environment variables
        let env_fn = lua.create_function(|lua_ctx, key: String| {
            match std::env::var(&key) {
                Ok(val) => Ok(LuaValue::String(lua_ctx.create_string(&val)?)),
                Err(_) => Ok(LuaValue::Nil),
            }
        })?;
        globals.set("env", env_fn)?;

        Ok(())
    }

    fn register_flow_api(lua: &Lua, registry: &NodeRegistry) -> Result<()> {
        let globals = lua.globals();

        // Create the Flow constructor: Flow.new(name)
        let flow_constructor = lua.create_table()?;
        let new_fn = lua.create_function(|lua, name: String| {
            let flow = lua.create_table()?;
            flow.set("_name", name)?;
            flow.set("_steps", lua.create_table()?)?;
            flow.set("_step_count", 0i32)?;

            // flow:step(name, node_config_or_function) -> step_builder
            let step_fn = lua.create_function(|lua, (flow_tbl, step_name, node_arg): (LuaTable, String, LuaValue)| {
                // Accept either a table (node config) or a function (auto-wrapped as code node)
                let node_config: LuaTable = match node_arg {
                    LuaValue::Table(tbl) => tbl,
                    LuaValue::Function(func) => {
                        let bytecode = func.dump(false);
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytecode);
                        let tbl = lua.create_table()?;
                        tbl.set("_node_type", "code")?;
                        tbl.set("bytecode_b64", b64)?;
                        tbl
                    }
                    _ => {
                        return Err(LuaError::RuntimeError(
                            "step() expects a node config table or a function".into(),
                        ));
                    }
                };
                let steps: LuaTable = flow_tbl.get("_steps")?;
                let count: i32 = flow_tbl.get("_step_count")?;

                let step = lua.create_table()?;
                step.set("name", step_name)?;
                step.set("node_type", node_config.get::<String>("_node_type")?)?;
                step.set("config", node_config)?;
                step.set("dependencies", lua.create_table()?)?;
                step.set("max_retries", 0)?;
                step.set("backoff_s", 1.0)?;
                step.set("timeout_s", LuaValue::Nil)?;
                step.set("route", LuaValue::Nil)?;

                steps.set(count + 1, step.clone())?;
                flow_tbl.set("_step_count", count + 1)?;

                // Return a step builder with chainable methods
                let builder = lua.create_table()?;
                builder.set("_step", step)?;

                // builder:depends_on(...)
                let depends_fn = lua.create_function(|_lua, args: LuaMultiValue| {
                    let mut iter = args.into_iter();
                    let builder: LuaTable = iter.next()
                        .ok_or_else(|| LuaError::RuntimeError("expected self".into()))?
                        .as_table()
                        .ok_or_else(|| LuaError::RuntimeError("expected table".into()))?
                        .clone();

                    let step: LuaTable = builder.get("_step")?;
                    let deps: LuaTable = step.get("dependencies")?;
                    let mut idx = deps.len()? as i32;

                    for arg in iter {
                        if let Some(dep) = arg.as_string().and_then(|s| s.to_str().ok().map(|s| s.to_string())) {
                            idx += 1;
                            deps.set(idx, dep)?;
                        }
                    }
                    Ok(builder)
                })?;
                builder.set("depends_on", depends_fn)?;

                // builder:retries(max, backoff)
                let retries_fn = lua.create_function(|_lua, (builder, max, backoff): (LuaTable, u32, Option<f64>)| {
                    let step: LuaTable = builder.get("_step")?;
                    step.set("max_retries", max)?;
                    if let Some(b) = backoff {
                        step.set("backoff_s", b)?;
                    }
                    Ok(builder)
                })?;
                builder.set("retries", retries_fn)?;

                // builder:timeout(seconds)
                let timeout_fn = lua.create_function(|_lua, (builder, seconds): (LuaTable, f64)| {
                    let step: LuaTable = builder.get("_step")?;
                    step.set("timeout_s", seconds)?;
                    Ok(builder)
                })?;
                builder.set("timeout", timeout_fn)?;

                // builder:route(route_name)
                let route_fn = lua.create_function(|_lua, (builder, route): (LuaTable, String)| {
                    let step: LuaTable = builder.get("_step")?;
                    step.set("route", route)?;
                    Ok(builder)
                })?;
                builder.set("route", route_fn)?;

                Ok(builder)
            })?;
            flow.set("step", step_fn)?;

            Ok(flow)
        })?;
        flow_constructor.set("new", new_fn)?;
        globals.set("Flow", flow_constructor)?;

        // Create the nodes table with factory functions for each registered node
        let nodes_table = lua.create_table()?;
        for (node_type, _desc) in registry.list() {
            let node_type_owned = node_type.to_string();
            let factory = lua.create_function(move |lua, config: Option<LuaTable>| {
                let tbl = config.unwrap_or(lua.create_table()?);
                tbl.set("_node_type", node_type_owned.clone())?;
                Ok(tbl)
            })?;
            nodes_table.set(node_type, factory)?;
        }
        globals.set("nodes", nodes_table)?;

        Ok(())
    }

    fn extract_flow(flow_table: &LuaTable) -> Result<FlowDefinition> {
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
                    m.insert("_step_name".to_string(), serde_json::Value::String(step_name.clone()));
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
            });
        }

        Ok(FlowDefinition { name, steps })
    }
}

/// Convert a Lua table to serde_json::Value.
fn lua_table_to_json(table: &LuaTable) -> Result<serde_json::Value> {
    // Check if this is an array (sequential integer keys starting from 1)
    let len = table.len()? as i64;
    let mut is_array = len > 0;

    if is_array {
        // Verify it's a proper sequence
        for i in 1..=len {
            if table.get::<LuaValue>(i).is_err() {
                is_array = false;
                break;
            }
        }
    }

    if is_array {
        let mut arr = Vec::new();
        for i in 1..=len {
            let val: LuaValue = table.get(i)?;
            arr.push(lua_value_to_json(&val)?);
        }
        Ok(serde_json::Value::Array(arr))
    } else {
        let mut map = serde_json::Map::new();
        for pair in table.pairs::<String, LuaValue>() {
            let (key, val) = pair?;
            map.insert(key, lua_value_to_json(&val)?);
        }
        Ok(serde_json::Value::Object(map))
    }
}

fn lua_value_to_json(value: &LuaValue) -> Result<serde_json::Value> {
    match value {
        LuaValue::Nil => Ok(serde_json::Value::Null),
        LuaValue::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        LuaValue::Integer(n) => Ok(serde_json::json!(*n)),
        LuaValue::Number(n) => Ok(serde_json::json!(*n)),
        LuaValue::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
        LuaValue::Table(t) => lua_table_to_json(t),
        _ => Ok(serde_json::Value::Null),
    }
}
