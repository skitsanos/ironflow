use anyhow::Result;
use mlua::prelude::*;

use crate::lua::analysis::HandlerDiagnostics;
use crate::nodes::NodeRegistry;

use super::context_keys::register_context_keys;
use super::handlers::serialize_handler;
use super::node_factories::register_node_factories;

/// Register the Flow API (`Flow.new`, `flow:step`, `flow:step_if`, `nodes`) into the Lua VM.
pub(super) fn register_flow_api(
    lua: &Lua,
    registry: &NodeRegistry,
    diagnostics: Option<HandlerDiagnostics>,
) -> Result<()> {
    let globals = lua.globals();

    // Create the Flow constructor: Flow.new(name)
    let flow_constructor = lua.create_table()?;
    let constructor_diagnostics = diagnostics.clone();
    let new_fn = lua.create_function(move |lua, name: String| {
        let flow = lua.create_table()?;
        flow.set("_name", name)?;
        flow.set("_steps", lua.create_table()?)?;
        flow.set("_step_count", 0i32)?;
        flow.set("step", step_function(lua, constructor_diagnostics.clone())?)?;
        flow.set(
            "step_if",
            step_if_function(lua, constructor_diagnostics.clone())?,
        )?;
        Ok(flow)
    })?;
    flow_constructor.set("new", new_fn)?;
    globals.set("Flow", flow_constructor)?;

    register_node_factories(lua, registry, diagnostics)?;

    Ok(())
}

/// `flow:step(name, node_config_or_function) -> step_builder`
fn step_function(lua: &Lua, diagnostics: Option<HandlerDiagnostics>) -> LuaResult<LuaFunction> {
    lua.create_function(
        move |lua, (flow_tbl, step_name, node_arg): (LuaTable, String, LuaValue)| {
            let node_config = node_config_from_arg(lua, node_arg, diagnostics.as_ref(), "step")?;
            let steps: LuaTable = flow_tbl.get("_steps")?;
            let count: i32 = flow_tbl.get("_step_count")?;

            let step = new_step(
                lua,
                step_name,
                node_config,
                lua.create_table()?,
                LuaValue::Nil,
            )?;
            steps.set(count + 1, step.clone())?;
            flow_tbl.set("_step_count", count + 1)?;

            // Return a step builder with chainable methods
            let builder = lua.create_table()?;
            builder.set("_step", step)?;
            attach_builder_methods(lua, &builder, "_step", true)?;
            register_context_keys(lua, &builder)?;
            Ok(builder)
        },
    )
}

/// `flow:step_if(condition, name, node_config_or_function) -> step_builder`
///
/// Syntactic sugar: creates an auto-named if_node guard plus the visible step
/// with depends_on(guard) and route("true").
fn step_if_function(lua: &Lua, diagnostics: Option<HandlerDiagnostics>) -> LuaResult<LuaFunction> {
    lua.create_function(
        move |lua,
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
            let guard_step = new_step(
                lua,
                guard_name.clone(),
                guard_config,
                lua.create_table()?,
                LuaValue::Nil,
            )?;
            steps.set(count + 1, guard_step.clone())?;

            // 2. Create the actual step with depends_on + route("true")
            let node_config =
                node_config_from_arg(lua, node_arg, diagnostics.as_ref(), "step_if")?;
            let deps = lua.create_table()?;
            deps.set(1, guard_name)?;
            let step = new_step(lua, step_name, node_config, deps, "true")?;
            steps.set(count + 2, step.clone())?;
            flow_tbl.set("_step_count", count + 2)?;

            // Return a builder for the actual step (chainable). User
            // dependencies must gate condition evaluation, so `depends_on`
            // extends the guard: the visible step already depends on it,
            // giving the order dependency -> guard -> visible step.
            let builder = lua.create_table()?;
            builder.set("_step", step)?;
            builder.set("_guard_step", guard_step)?;
            attach_builder_methods(lua, &builder, "_guard_step", false)?;
            register_context_keys(lua, &builder)?;
            Ok(builder)
        },
    )
}

/// Accept either a table (node config) or a function (auto-wrapped as a code node).
fn node_config_from_arg(
    lua: &Lua,
    node_arg: LuaValue,
    diagnostics: Option<&HandlerDiagnostics>,
    function_name: &str,
) -> LuaResult<LuaTable> {
    match node_arg {
        LuaValue::Table(tbl) => Ok(tbl),
        LuaValue::Function(func) => {
            let b64 = serialize_handler(&func, diagnostics)?;
            let tbl = lua.create_table()?;
            tbl.set("_node_type", "code")?;
            tbl.set("bytecode_b64", b64)?;
            Ok(tbl)
        }
        _ => Err(LuaError::RuntimeError(format!(
            "{function_name}() expects a node config table or a function"
        ))),
    }
}

/// Build one step table with the shared defaults.
fn new_step(
    lua: &Lua,
    name: String,
    node_config: LuaTable,
    dependencies: LuaTable,
    route: impl IntoLua,
) -> LuaResult<LuaTable> {
    let step = lua.create_table()?;
    step.set("name", name)?;
    step.set("node_type", node_config.get::<String>("_node_type")?)?;
    step.set("config", node_config)?;
    step.set("dependencies", dependencies)?;
    step.set("max_retries", 0)?;
    step.set("backoff_s", 1.0)?;
    step.set("timeout_s", LuaValue::Nil)?;
    step.set("route", route)?;
    Ok(step)
}

/// Attach the chainable builder methods. `dependency_target` names the step
/// table whose dependency list `depends_on` extends; `step_if` fixes its route
/// and therefore omits the `route` method.
fn attach_builder_methods(
    lua: &Lua,
    builder: &LuaTable,
    dependency_target: &'static str,
    with_route: bool,
) -> LuaResult<()> {
    // builder:depends_on(...)
    let depends_fn = lua.create_function(move |_lua, args: LuaMultiValue| {
        let mut iter = args.into_iter();
        let builder: LuaTable = iter
            .next()
            .ok_or_else(|| LuaError::RuntimeError("expected self".into()))?
            .as_table()
            .ok_or_else(|| LuaError::RuntimeError("expected table".into()))?
            .clone();

        let target: LuaTable = builder.get(dependency_target)?;
        let deps: LuaTable = target.get("dependencies")?;
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
    let timeout_fn = lua.create_function(|_lua, (builder, seconds): (LuaTable, f64)| {
        let step: LuaTable = builder.get("_step")?;
        step.set("timeout_s", seconds)?;
        Ok(builder)
    })?;
    builder.set("timeout", timeout_fn)?;

    // builder:route(route_name)
    if with_route {
        let route_fn = lua.create_function(|_lua, (builder, route): (LuaTable, String)| {
            let step: LuaTable = builder.get("_step")?;
            step.set("route", route)?;
            Ok(builder)
        })?;
        builder.set("route", route_fn)?;
    }

    // builder:on_error(step_name)
    let on_error_fn = lua.create_function(|_lua, (builder, step_name): (LuaTable, String)| {
        let step: LuaTable = builder.get("_step")?;
        step.set("on_error", step_name)?;
        Ok(builder)
    })?;
    builder.set("on_error", on_error_fn)?;

    Ok(())
}
