use anyhow::Result;
use base64::Engine;
use mlua::prelude::*;

use crate::nodes::NodeRegistry;

/// Register the Flow API (`Flow.new`, `flow:step`, `flow:step_if`, `nodes`) into the Lua VM.
pub(super) fn register_flow_api(lua: &Lua, registry: &NodeRegistry) -> Result<()> {
    let globals = lua.globals();

    // Create the Flow constructor: Flow.new(name)
    let flow_constructor = lua.create_table()?;
    let new_fn = lua.create_function(|lua, name: String| {
        let flow = lua.create_table()?;
        flow.set("_name", name)?;
        flow.set("_steps", lua.create_table()?)?;
        flow.set("_step_count", 0i32)?;

        // flow:step(name, node_config_or_function) -> step_builder
        let step_fn = lua.create_function(
            |lua, (flow_tbl, step_name, node_arg): (LuaTable, String, LuaValue)| {
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
                    let builder: LuaTable = iter
                        .next()
                        .ok_or_else(|| LuaError::RuntimeError("expected self".into()))?
                        .as_table()
                        .ok_or_else(|| LuaError::RuntimeError("expected table".into()))?
                        .clone();

                    let step: LuaTable = builder.get("_step")?;
                    let deps: LuaTable = step.get("dependencies")?;
                    let mut idx = deps.len()? as i32;

                    for arg in iter {
                        if let Some(dep) = arg
                            .as_string()
                            .and_then(|s| s.to_str().ok().map(|s| s.to_string()))
                        {
                            idx += 1;
                            deps.set(idx, dep)?;
                        }
                    }
                    Ok(builder)
                })?;
                builder.set("depends_on", depends_fn)?;

                // builder:retries(max, backoff)
                let retries_fn = lua.create_function(
                    |_lua, (builder, max, backoff): (LuaTable, u32, Option<f64>)| {
                        let step: LuaTable = builder.get("_step")?;
                        step.set("max_retries", max)?;
                        if let Some(b) = backoff {
                            step.set("backoff_s", b)?;
                        }
                        Ok(builder)
                    },
                )?;
                builder.set("retries", retries_fn)?;

                // builder:timeout(seconds)
                let timeout_fn =
                    lua.create_function(|_lua, (builder, seconds): (LuaTable, f64)| {
                        let step: LuaTable = builder.get("_step")?;
                        step.set("timeout_s", seconds)?;
                        Ok(builder)
                    })?;
                builder.set("timeout", timeout_fn)?;

                // builder:route(route_name)
                let route_fn =
                    lua.create_function(|_lua, (builder, route): (LuaTable, String)| {
                        let step: LuaTable = builder.get("_step")?;
                        step.set("route", route)?;
                        Ok(builder)
                    })?;
                builder.set("route", route_fn)?;

                // builder:on_error(step_name)
                let on_error_fn =
                    lua.create_function(|_lua, (builder, step_name): (LuaTable, String)| {
                        let step: LuaTable = builder.get("_step")?;
                        step.set("on_error", step_name)?;
                        Ok(builder)
                    })?;
                builder.set("on_error", on_error_fn)?;

                Ok(builder)
            },
        )?;
        flow.set("step", step_fn)?;

        // flow:step_if(condition, name, node_config_or_function) -> step_builder
        // Syntactic sugar: creates an auto-named if_node + the actual step
        // with depends_on(if_node) and route("true").
        let step_if_fn =
            lua.create_function(
                |lua,
                 (flow_tbl, condition, step_name, node_arg): (
                    LuaTable,
                    String,
                    String,
                    LuaValue,
                )| {
                    // 1. Create a hidden if_node step
                    let guard_name = format!("_if_{}", step_name);
                    let steps: LuaTable = flow_tbl.get("_steps")?;
                    let count: i32 = flow_tbl.get("_step_count")?;

                    let guard_config = lua.create_table()?;
                    guard_config.set("_node_type", "if_node")?;
                    guard_config.set("condition", condition)?;

                    let guard_step = lua.create_table()?;
                    guard_step.set("name", guard_name.clone())?;
                    guard_step.set("node_type", "if_node")?;
                    guard_step.set("config", guard_config)?;
                    guard_step.set("dependencies", lua.create_table()?)?;
                    guard_step.set("max_retries", 0)?;
                    guard_step.set("backoff_s", 1.0)?;
                    guard_step.set("timeout_s", LuaValue::Nil)?;
                    guard_step.set("route", LuaValue::Nil)?;

                    steps.set(count + 1, guard_step)?;

                    // 2. Create the actual step with depends_on + route("true")
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
                                "step_if() expects a node config table or a function".into(),
                            ));
                        }
                    };

                    let deps = lua.create_table()?;
                    deps.set(1, guard_name)?;

                    let step = lua.create_table()?;
                    step.set("name", step_name)?;
                    step.set("node_type", node_config.get::<String>("_node_type")?)?;
                    step.set("config", node_config)?;
                    step.set("dependencies", deps)?;
                    step.set("max_retries", 0)?;
                    step.set("backoff_s", 1.0)?;
                    step.set("timeout_s", LuaValue::Nil)?;
                    step.set("route", "true")?;

                    steps.set(count + 2, step.clone())?;
                    flow_tbl.set("_step_count", count + 2)?;

                    // Return a builder for the actual step (chainable)
                    let builder = lua.create_table()?;
                    builder.set("_step", step)?;

                    let depends_fn = lua.create_function(|_lua, args: LuaMultiValue| {
                        let mut iter = args.into_iter();
                        let builder: LuaTable = iter
                            .next()
                            .ok_or_else(|| LuaError::RuntimeError("expected self".into()))?
                            .as_table()
                            .ok_or_else(|| LuaError::RuntimeError("expected table".into()))?
                            .clone();

                        let step: LuaTable = builder.get("_step")?;
                        let deps: LuaTable = step.get("dependencies")?;
                        let mut idx = deps.len()? as i32;

                        for arg in iter {
                            if let Some(dep) = arg
                                .as_string()
                                .and_then(|s| s.to_str().ok().map(|s| s.to_string()))
                            {
                                idx += 1;
                                deps.set(idx, dep)?;
                            }
                        }
                        Ok(builder)
                    })?;
                    builder.set("depends_on", depends_fn)?;

                    let retries_fn = lua.create_function(
                        |_lua, (builder, max, backoff): (LuaTable, u32, Option<f64>)| {
                            let step: LuaTable = builder.get("_step")?;
                            step.set("max_retries", max)?;
                            if let Some(b) = backoff {
                                step.set("backoff_s", b)?;
                            }
                            Ok(builder)
                        },
                    )?;
                    builder.set("retries", retries_fn)?;

                    let timeout_fn =
                        lua.create_function(|_lua, (builder, seconds): (LuaTable, f64)| {
                            let step: LuaTable = builder.get("_step")?;
                            step.set("timeout_s", seconds)?;
                            Ok(builder)
                        })?;
                    builder.set("timeout", timeout_fn)?;

                    let on_error_fn =
                        lua.create_function(|_lua, (builder, step_name): (LuaTable, String)| {
                            let step: LuaTable = builder.get("_step")?;
                            step.set("on_error", step_name)?;
                            Ok(builder)
                        })?;
                    builder.set("on_error", on_error_fn)?;

                    Ok(builder)
                },
            )?;
        flow.set("step_if", step_if_fn)?;

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

            // For code: if `source` is a function, serialize to bytecode
            if node_type_owned == "code"
                && let Ok(LuaValue::Function(func)) = tbl.get::<LuaValue>("source")
            {
                let bytecode = func.dump(false);
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytecode);
                tbl.set("bytecode_b64", b64)?;
                tbl.set("source", LuaValue::Nil)?;
            }

            // For foreach: if `transform` is a function, serialize to bytecode
            if node_type_owned == "foreach"
                && let Ok(LuaValue::Function(func)) = tbl.get::<LuaValue>("transform")
            {
                let bytecode = func.dump(false);
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytecode);
                tbl.set("transform_bytecode_b64", b64)?;
                tbl.set("transform", LuaValue::Nil)?;
            }

            Ok(tbl)
        })?;
        nodes_table.set(node_type, factory)?;
    }
    globals.set("nodes", nodes_table)?;

    Ok(())
}
