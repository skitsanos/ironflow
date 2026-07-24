// IF-047: an opt-in run-level deadline (IRONFLOW_MAX_RUN_SECONDS) reclaims a run
// whose node hangs without its own step timeout, terminalizing it as Cancelled.
//
// Dedicated test binary: it mutates a process-global env var.

use std::collections::HashMap;
use std::sync::Arc;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::RunStatus;
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::json_store::JsonStateStore;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_level_deadline_cancels_a_long_running_step() {
    unsafe {
        std::env::set_var("IRONFLOW_MAX_RUN_SECONDS", "1");
    }

    let registry = Arc::new(NodeRegistry::with_builtins());
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(dir.path()));

    // A step that would sleep for 30s, far beyond the 1s run deadline and with
    // no per-step timeout of its own.
    let source = r#"
        local flow = Flow.new("slow")
        flow:step("wait", nodes.delay({ seconds = 30 }))
        return flow
    "#;
    let flow = LuaRuntime::load_flow_from_string(source, &registry).unwrap();
    let engine = WorkflowEngine::new(registry, store.clone(), None);

    let run_id = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        engine.execute(&flow, HashMap::new()),
    )
    .await
    .expect("the run-level deadline must terminate the run, not hang")
    .unwrap();

    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(
        info.status,
        RunStatus::Cancelled,
        "a run past its deadline must be cancelled"
    );

    unsafe {
        std::env::remove_var("IRONFLOW_MAX_RUN_SECONDS");
    }
}
