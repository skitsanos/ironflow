use std::path::Path;
use std::sync::Arc;

use ironflow::engine::types::RunStatus;
use ironflow::nodes::NodeRegistry;
use ironflow::scheduler::execution::FlowExecutor;
use ironflow::storage::StateStore as _;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;

#[allow(dead_code)] // Each integration-test crate uses a different subset.
pub struct TestScheduler {
    pub executor: Arc<FlowExecutor>,
    pub store: Arc<JsonStateStore>,
    pub events: Arc<MemoryEventStore>,
    pub store_dir: tempfile::TempDir,
}

#[allow(dead_code)]
pub fn build_executor(flows_dir: &Path) -> TestScheduler {
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(store_dir.path()));
    let events = Arc::new(MemoryEventStore::new());
    let executor = Arc::new(FlowExecutor::new(
        Arc::new(NodeRegistry::with_builtins()),
        store.clone(),
        events.clone(),
        Some(flows_dir.to_path_buf()),
        None,
    ));

    TestScheduler {
        executor,
        store,
        events,
        store_dir,
    }
}

/// Poll a run to a terminal status. `FlowExecutor::run` now starts a run and
/// returns without awaiting its completion, so tests asserting on final state
/// must settle first rather than checking immediately after `run()` returns.
#[allow(dead_code)]
pub async fn wait_for_terminal(store: &JsonStateStore, run_id: &str) -> RunStatus {
    for _ in 0..100 {
        let info = store.get_run_info(run_id).await.unwrap();
        if info.status.is_terminal() {
            return info.status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("run {run_id} did not reach a terminal status in time");
}

#[allow(dead_code)]
pub fn flows_with_logger() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("nightly.lua"),
        r#"
        local flow = Flow.new("nightly_report")
        flow:step("emit", nodes.log({ message = "nightly ran" }))
        return flow
        "#,
    )
    .unwrap();
    dir
}
