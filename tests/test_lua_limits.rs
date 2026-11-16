use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::util::limits::{LuaExecutionLimits, apply_lua_limits};
use mlua::prelude::*;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn set_lua_limit_env() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = ENV_LOCK.lock().await;
    unsafe {
        std::env::set_var("IRONFLOW_LUA_MAX_INSTRUCTIONS", "1000");
        std::env::set_var("IRONFLOW_LUA_MAX_SECONDS", "0");
        std::env::set_var("IRONFLOW_LUA_MAX_MEMORY_BYTES", "0");
        std::env::set_var("IRONFLOW_LUA_HOOK_INTERVAL", "10");
        std::env::set_var("IRONFLOW_LUA_GC_AFTER_EXECUTION", "true");
    }
    guard
}

fn clear_lua_limit_env() {
    unsafe {
        std::env::remove_var("IRONFLOW_LUA_MAX_INSTRUCTIONS");
        std::env::remove_var("IRONFLOW_LUA_MAX_SECONDS");
        std::env::remove_var("IRONFLOW_LUA_MAX_MEMORY_BYTES");
        std::env::remove_var("IRONFLOW_LUA_HOOK_INTERVAL");
        std::env::remove_var("IRONFLOW_LUA_GC_AFTER_EXECUTION");
    }
}

#[test]
fn lua_instruction_hook_stops_infinite_loop() {
    let lua = Lua::new();
    apply_lua_limits(
        &lua,
        LuaExecutionLimits {
            max_instructions: Some(1000),
            max_seconds: None,
            max_memory_bytes: None,
            max_pattern_steps: None,
            hook_interval: 10,
            gc_after_execution: true,
        },
    )
    .unwrap();

    let err = lua
        .load("while true do end")
        .exec()
        .expect_err("infinite Lua loop must be interrupted");

    assert!(
        err.to_string().contains("instruction budget"),
        "expected instruction budget error, got: {err}"
    );
}

#[tokio::test]
async fn flow_loading_stops_infinite_top_level_lua() {
    let _guard = set_lua_limit_env().await;
    let registry = NodeRegistry::with_builtins();

    let err = LuaRuntime::load_flow_from_string("while true do end", &registry)
        .expect_err("flow parser must interrupt infinite Lua");

    assert!(
        err.to_string().contains("instruction budget"),
        "expected instruction budget error, got: {err}"
    );

    clear_lua_limit_env();
}

#[tokio::test]
async fn code_node_stops_infinite_lua_source() {
    let _guard = set_lua_limit_env().await;
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("code").unwrap();

    let err = node
        .execute(
            &serde_json::json!({
                "source": "while true do end",
            }),
            &Default::default(),
        )
        .await
        .expect_err("code node must interrupt infinite Lua");

    assert!(
        err.to_string().contains("instruction budget"),
        "expected instruction budget error, got: {err}"
    );

    clear_lua_limit_env();
}

/// IF-213: the pattern-matching step budget is read from the environment and
/// applies to code nodes and to the top-level code of a flow.
#[tokio::test]
async fn pattern_step_budget_override_stops_a_backtracking_pattern() {
    let _guard = ENV_LOCK.lock().await;
    unsafe {
        std::env::set_var("IRONFLOW_LUA_MAX_PATTERN_STEPS", "20000");
    }
    let registry = NodeRegistry::with_builtins();
    let pathological = r#"string.find(string.rep("a", 2000), "^.-.-.-b")"#;
    let expected =
        "Lua pattern matching exceeded step budget of 20000 (IRONFLOW_LUA_MAX_PATTERN_STEPS)";

    let err = registry
        .get("code")
        .unwrap()
        .execute(
            &serde_json::json!({ "source": format!("return {{ found = {pathological} }}") }),
            &Default::default(),
        )
        .await
        .expect_err("code node must stop a backtracking pattern");
    assert!(err.to_string().contains(expected), "{err}");

    let err =
        LuaRuntime::load_flow_from_string(&format!("local found = {pathological}"), &registry)
            .expect_err("flow loading must stop a backtracking pattern");
    assert!(err.to_string().contains(expected), "{err}");

    // `0` disables the step budget; the match then runs to its end.
    unsafe {
        std::env::set_var("IRONFLOW_LUA_MAX_PATTERN_STEPS", "0");
    }
    let output = registry
        .get("code")
        .unwrap()
        .execute(
            &serde_json::json!({
                "source": r#"return { found = string.find(string.rep("a", 300), "^.-.-.-b") == nil }"#
            }),
            &Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(output["found"], serde_json::json!(true));

    unsafe {
        std::env::set_var("IRONFLOW_LUA_MAX_PATTERN_STEPS", "many");
    }
    let err = ironflow::util::limits::validate_environment().unwrap_err();
    assert!(
        err.to_string()
            .contains("IRONFLOW_LUA_MAX_PATTERN_STEPS must be a non-negative integer"),
        "{err}"
    );

    unsafe {
        std::env::remove_var("IRONFLOW_LUA_MAX_PATTERN_STEPS");
    }
}
