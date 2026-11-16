//! IF-215: a step's retry count has a fixed ceiling, so the attempt events a
//! failing step can store (up to 64 KiB each) are bounded.

use std::sync::Arc;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{Context, RunStatus};
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::null_store::NullStateStore;

/// The fixed ceiling on `retries(max, backoff)`.
const MAX_RETRIES: u32 = 100;

fn failing_flow(dir: &std::path::Path, retries: &str) -> std::path::PathBuf {
    let path = dir.join("flow.lua");
    std::fs::write(
        &path,
        format!(
            r#"
            local flow = Flow.new("retry-ceiling")
            flow:step("fails", nodes.code({{ source = "error('boom')" }})):retries({retries}, 0)
            return flow
            "#
        ),
    )
    .unwrap();
    path
}

#[tokio::test]
async fn a_retry_count_above_the_ceiling_is_refused_before_any_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let registry = Arc::new(NodeRegistry::with_builtins());
    let path = failing_flow(dir.path(), &(MAX_RETRIES + 1).to_string());
    let flow = LuaRuntime::load_flow(path.to_str().unwrap(), &registry).unwrap();

    assert_eq!(
        flow.validate_dag(),
        ["Step 'fails' retry count is too large (max 100)"]
    );
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(registry, store, None);
    let error = match engine.start(&flow, Context::new()).await {
        Ok(_) => panic!("a retry count above the ceiling must not start a run"),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        error.contains("Step 'fails' retry count is too large (max 100)"),
        "{error}"
    );
}

#[tokio::test]
async fn the_ceiling_itself_runs_and_stops_after_its_last_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let registry = Arc::new(NodeRegistry::with_builtins());
    let path = failing_flow(dir.path(), &MAX_RETRIES.to_string());
    let flow = LuaRuntime::load_flow(path.to_str().unwrap(), &registry).unwrap();
    assert!(flow.validate_dag().is_empty());

    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(registry, store.clone(), None);
    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    handle.wait().await.unwrap();

    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.tasks["fails"].attempt, MAX_RETRIES + 1);
}

#[test]
fn a_retry_count_that_is_not_a_u32_fails_to_load() {
    let dir = tempfile::tempdir().unwrap();
    let registry = NodeRegistry::with_builtins();
    for retries in ["4294967296", "-1", "1e30"] {
        let path = failing_flow(dir.path(), retries);
        assert!(
            LuaRuntime::load_flow(path.to_str().unwrap(), &registry).is_err(),
            "retries({retries}) must not load"
        );
    }
}
