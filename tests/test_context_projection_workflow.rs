use std::sync::Arc;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{Context, RunStatus, TaskStatus};
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::null_store::NullStateStore;
use serde_json::json;

const EXAMPLE: &str = include_str!("../examples/07-advanced/context_projection.lua");

#[tokio::test]
async fn context_projection_example_runs_after_a_large_native_node_output() {
    let registry = Arc::new(NodeRegistry::with_builtins());
    let loaded = LuaRuntime::validate_flow_from_string(EXAMPLE, &registry).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(registry, store.clone(), None);
    let run_id = engine.execute(&loaded.flow, Context::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx["unused"].as_array().unwrap().len(), 200_000);
    assert_eq!(info.ctx["inspected_count"], 200_000);
    assert_eq!(info.ctx["ready"], true);
    assert_eq!(info.tasks["inspect"].status, TaskStatus::Success);
    assert_eq!(info.tasks["constant"].status, TaskStatus::Success);
}

#[tokio::test]
async fn selecting_the_large_native_output_still_fails_the_handler() {
    let registry = Arc::new(NodeRegistry::with_builtins());
    let source = EXAMPLE
        .replace("assert(ctx.unused == nil)", "assert(ctx.unused[1] == 1)")
        .replace(
            "context_keys({\"count\"})",
            "context_keys({\"count\", \"unused\"})",
        );
    let flow = LuaRuntime::load_flow_from_string(&source, &registry).unwrap();
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(registry, store.clone(), None);
    let run_id = engine.execute(&flow, Context::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.tasks["parse"].status, TaskStatus::Success);
    assert_eq!(info.tasks["inspect"].status, TaskStatus::Failed);
    let error = info.tasks["inspect"].error.as_deref().unwrap();
    assert!(error.contains("JSON-to-Lua maximum node count"), "{error}");
    assert!(error.contains("$.unused["), "{error}");
}

#[tokio::test]
async fn context_projection_example_keeps_engine_keys_injected_by_the_cli() {
    let registry = Arc::new(NodeRegistry::with_builtins());
    let flow = LuaRuntime::load_flow_from_string(EXAMPLE, &registry).unwrap();
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(registry, store.clone(), None);
    // `ironflow run` injects `_flow_dir`; an empty projection must not hide it
    // and the example's empty-snapshot step must still succeed.
    let initial = Context::from([("_flow_dir".into(), json!("examples/07-advanced"))]);
    let run_id = engine.execute(&flow, initial).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Success, "{:?}", info.tasks);
    assert_eq!(info.tasks["constant"].status, TaskStatus::Success);
    assert_eq!(info.ctx["_flow_dir"], "examples/07-advanced");
    assert_eq!(info.ctx["ready"], true);
}
