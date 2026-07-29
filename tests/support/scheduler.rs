use std::path::Path;
use std::sync::Arc;

use ironflow::nodes::NodeRegistry;
use ironflow::scheduler::execution::FlowExecutor;
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
