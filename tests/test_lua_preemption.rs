use std::time::Duration;

use ironflow::engine::types::Context;
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::util::execution::{run_blocking_step, with_execution_deadline};
use ironflow::util::limits::{LuaExecutionLimits, apply_lua_limits_with_control};
use mlua::prelude::*;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const LUA_LIMIT_ENV: &[(&str, &str)] = &[
    ("IRONFLOW_LUA_MAX_INSTRUCTIONS", "1000000000000"),
    ("IRONFLOW_LUA_MAX_SECONDS", "60"),
    ("IRONFLOW_LUA_MAX_MEMORY_BYTES", "0"),
    ("IRONFLOW_LUA_HOOK_INTERVAL", "1000"),
];

struct LuaLimitEnv;

impl LuaLimitEnv {
    fn permissive() -> Self {
        for (key, value) in LUA_LIMIT_ENV {
            // SAFETY: tests in this integration-test process serialize all
            // environment mutation through `ENV_LOCK`.
            unsafe { std::env::set_var(key, value) };
        }
        Self
    }
}

impl Drop for LuaLimitEnv {
    fn drop(&mut self) {
        for (key, _) in LUA_LIMIT_ENV {
            // SAFETY: `LuaLimitEnv` is created only while `ENV_LOCK` is held.
            unsafe { std::env::remove_var(key) };
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn dropped_lua_waiter_cancels_the_blocking_vm_without_starving_tokio() {
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let operation = run_blocking_step(move |execution| {
        let lua = Lua::new();
        apply_lua_limits_with_control(
            &lua,
            LuaExecutionLimits {
                max_instructions: None,
                max_seconds: None,
                max_memory_bytes: None,
                hook_interval: 1_000,
                gc_after_execution: false,
            },
            Some(execution),
        )?;

        let error = lua
            .load("while true do end")
            .exec()
            .expect_err("dropped waiter must interrupt Lua");
        let _ = finished_tx.send(error.to_string());
        Ok(())
    });

    tokio::time::timeout(Duration::from_millis(40), operation)
        .await
        .expect_err("the async timeout must remain responsive");

    let error = tokio::time::timeout(Duration::from_secs(1), finished_rx)
        .await
        .expect("blocking Lua worker did not stop")
        .expect("blocking Lua worker dropped its result");
    assert!(error.contains("step execution cancelled"), "{error}");
}

#[tokio::test]
async fn code_infinite_loop_observes_the_scoped_step_deadline() {
    let _lock = ENV_LOCK.lock().await;
    let _env = LuaLimitEnv::permissive();
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("code").unwrap();
    let config = serde_json::json!({ "source": "while true do end" });
    let deadline = tokio::time::Instant::now() + Duration::from_millis(40);

    let error = tokio::time::timeout(
        Duration::from_secs(2),
        with_execution_deadline(Some(deadline), node.execute(&config, &Context::new())),
    )
    .await
    .expect("code node did not stop at its deadline")
    .expect_err("infinite code must fail");

    assert!(
        error.to_string().contains("step deadline exceeded"),
        "{error:#}"
    );
}

#[tokio::test]
async fn foreach_infinite_transform_observes_the_scoped_step_deadline() {
    let _lock = ENV_LOCK.lock().await;
    let _env = LuaLimitEnv::permissive();
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("foreach").unwrap();
    let flow = LuaRuntime::load_flow_from_string(
        r#"
        local flow = Flow.new("deadline")
        flow:step("loop", nodes.foreach({
            source_key = "items",
            transform = function() while true do end end
        }))
        return flow
        "#,
        &registry,
    )
    .unwrap();
    let config = &flow.steps[0].config;
    let ctx = Context::from([("items".to_string(), serde_json::json!([1]))]);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(40);

    let error = tokio::time::timeout(
        Duration::from_secs(2),
        with_execution_deadline(Some(deadline), node.execute(config, &ctx)),
    )
    .await
    .expect("foreach node did not stop at its deadline")
    .expect_err("infinite transform must fail");

    assert!(
        error.to_string().contains("step deadline exceeded"),
        "{error:#}"
    );
}

#[tokio::test]
async fn async_flow_loader_observes_the_enclosing_step_deadline() {
    let _lock = ENV_LOCK.lock().await;
    let _env = LuaLimitEnv::permissive();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("infinite.lua");
    std::fs::write(&path, "while true do end").unwrap();
    let registry = NodeRegistry::with_builtins();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(40);

    let error = tokio::time::timeout(
        Duration::from_secs(2),
        with_execution_deadline(
            Some(deadline),
            LuaRuntime::load_flow_async(path.to_str().unwrap(), &registry),
        ),
    )
    .await
    .expect("async flow loader did not stop at its deadline")
    .expect_err("infinite top-level Lua must fail");

    assert!(
        error.to_string().contains("step deadline exceeded"),
        "{error:#}"
    );
}
