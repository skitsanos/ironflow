use std::sync::Arc;
use std::time::Duration;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{Context, RunStatus};
use ironflow::lua::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::{StateStore, json_store::JsonStateStore};

const EXAMPLE: &str = include_str!("../examples/07-advanced/reused_callbacks.lua");

async fn run(source: &str) -> Context {
    let registry = Arc::new(NodeRegistry::with_builtins());
    let flow = LuaRuntime::load_flow_from_string_async(source, &registry)
        .await
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(directory.path()));
    let engine = WorkflowEngine::new(registry, store.clone(), None);
    let id = tokio::time::timeout(
        Duration::from_secs(10),
        engine.execute(&flow, Context::new()),
    )
    .await
    .expect("handler workflow did not settle")
    .unwrap();
    let info = store.get_run_info(&id).await.unwrap();
    assert_eq!(info.status, RunStatus::Success, "{info:?}");
    info.ctx
}

#[tokio::test]
async fn one_callback_validates_and_runs_across_all_function_entry_points() {
    assert_eq!(run(EXAMPLE).await["reused_callbacks_verified"], true);
    let registry = NodeRegistry::with_builtins();
    let validated = LuaRuntime::validate_flow_from_string(EXAMPLE, &registry).unwrap();
    assert!(validated.warnings.is_empty());
    assert_eq!(validated.flow.steps.len(), 7);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reused.lua");
    std::fs::write(&path, EXAMPLE).unwrap();
    let path = path.to_str().unwrap();
    assert!(
        LuaRuntime::validate_flow(path, &registry)
            .unwrap()
            .warnings
            .is_empty()
    );
    assert!(
        LuaRuntime::validate_flow_async(path, &registry)
            .await
            .unwrap()
            .warnings
            .is_empty()
    );
    assert!(
        LuaRuntime::validate_flow_from_string_async(EXAMPLE, &registry)
            .await
            .unwrap()
            .warnings
            .is_empty()
    );
}

#[tokio::test]
async fn distinct_callbacks_can_be_registered_out_of_source_order() {
    let source = r#"local flow = Flow.new("distinct")
local first = function(ctx)
    return { count = ctx.count + 1 }
end
local second = function()
    return { count = 10 }
end
flow:step("second", second)
flow:step("first", first):depends_on("second")
return flow"#;
    let registry = NodeRegistry::with_builtins();
    assert!(
        LuaRuntime::validate_flow_from_string(source, &registry)
            .unwrap()
            .warnings
            .is_empty()
    );
    assert_eq!(run(source).await["count"], 11);
}

#[tokio::test]
async fn separate_closures_from_one_source_function_can_be_registered_in_a_loop() {
    let source = r#"local flow = Flow.new("factory")
local function factory()
    return function(ctx)
        return { count = (ctx.count or 0) + 1 }
    end
end
local previous
for index = 1, 4 do
    local name = "step" .. index
    local step = flow:step(name, factory())
    if previous then step:depends_on(previous) end
    previous = name
end
return flow"#;
    assert_eq!(run(source).await["count"], 4);
    let registry = NodeRegistry::with_builtins();
    assert!(
        LuaRuntime::validate_flow_from_string(source, &registry)
            .unwrap()
            .warnings
            .is_empty()
    );
}

#[test]
fn reused_handler_warnings_keep_source_positions_and_are_deduplicated() {
    let source = r#"local flow = Flow.new("warnings")
local function unused() return missing_unused end
local function shared(ctx)
    return { value = missing_shared, context = ctx }
end
flow:step("one", shared)
flow:step("two", shared):depends_on("one")
flow:step("three", nodes.code({ source = shared })):depends_on("two")
flow:step("four", nodes.foreach({ source_key = "items", transform = shared }))
return flow"#;
    let registry = NodeRegistry::with_builtins();
    let validated = LuaRuntime::validate_flow_from_string(source, &registry).unwrap();
    assert_eq!(validated.warnings.len(), 1);
    let warning = &validated.warnings[0];
    assert_eq!(warning.code, "undefined_global");
    assert_eq!((warning.line, warning.column), (4, 22));
    assert_eq!(warning.step, None);
    assert!(warning.message.contains("`missing_shared`"));
}

#[test]
fn distinct_handler_warnings_are_not_merged_or_associated_by_registration_order() {
    let source = r#"local flow = Flow.new("distinct_warnings")
local function first() return missing_first end
local function second() return missing_second end
flow:step("two", second)
flow:step("one", first)
flow:step("again", second)
return flow"#;
    let registry = NodeRegistry::with_builtins();
    let warnings = LuaRuntime::validate_flow_from_string(source, &registry)
        .unwrap()
        .warnings;
    assert_eq!(warnings.len(), 2);
    assert_eq!((warnings[0].line, warnings[1].line), (2, 3));
    assert!(warnings[0].message.contains("`missing_first`"));
    assert!(warnings[1].message.contains("`missing_second`"));
}

#[test]
fn reused_captured_local_is_rejected_by_loading_and_validation_at_every_entry_point() {
    let registry = NodeRegistry::with_builtins();
    for registration in [
        "flow:step(name, shared)",
        "flow:step_if('true', name, shared)",
        "flow:step(name, nodes.code({ source = shared }))",
        "flow:step(name, nodes.foreach({ source_key = 'items', transform = shared }))",
    ] {
        let source = format!(
            "local flow = Flow.new('captures')\nlocal captured = 42\nlocal function shared() return captured end\nfor _, name in ipairs({{'one', 'two'}}) do {registration} end\nreturn flow"
        );
        for error in [
            LuaRuntime::load_flow_from_string(&source, &registry).unwrap_err(),
            LuaRuntime::validate_flow_from_string(&source, &registry).unwrap_err(),
        ] {
            assert!(
                format!("{error:#}").contains("captures 1 outer local value"),
                "{error:#}"
            );
        }
    }
}

#[test]
fn ambiguous_source_ranges_are_not_guessed_even_when_only_one_function_is_registered() {
    let registry = NodeRegistry::with_builtins();
    for selected in ["first", "second"] {
        let source = format!(
            "local flow = Flow.new('ambiguous')\nlocal first = function() return missing_first end; local second = function() return {{ ok = true }} end\nflow:step('selected', {selected})\nreturn flow"
        );
        assert!(LuaRuntime::load_flow_from_string(&source, &registry).is_ok());
        let error = LuaRuntime::validate_flow_from_string(&source, &registry).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("Ambiguous serialized Lua handler at lines 2-2"),
            "{message}"
        );
        assert!(message.contains("distinct lines"), "{message}");
    }
}

#[tokio::test]
async fn invalid_callback_still_fails_execution_after_non_strict_validation() {
    let source = r#"local flow = Flow.new("invalid")
local function shared() return { value = missing_shared() } end
flow:step("one", shared)
flow:step("two", shared):depends_on("one")
return flow"#;
    let registry = NodeRegistry::with_builtins();
    let validated = LuaRuntime::validate_flow_from_string(source, &registry).unwrap();
    assert_eq!(validated.warnings.len(), 1);
    let step = &validated.flow.steps[0];
    let error = registry
        .get("code")
        .unwrap()
        .execute(&step.config, &Context::new())
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("missing_shared"));
    assert!(step.config.get("bytecode_b64").is_some());
}
