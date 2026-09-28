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
    build_executor_with_registry(flows_dir, NodeRegistry::with_builtins())
}

fn build_executor_with_registry(flows_dir: &Path, registry: NodeRegistry) -> TestScheduler {
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(store_dir.path()));
    let events = Arc::new(MemoryEventStore::new());
    let executor = Arc::new(FlowExecutor::new(
        Arc::new(registry),
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
    // 300 * 20ms = 6s: comfortably above the longest delay step any test
    // flow in this suite uses (3s), with margin for scheduling overhead.
    for _ in 0..300 {
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

/// A real engine task whose completion is controlled by the test, independent
/// of wall-clock speed. Override only this fixture's delay node.
#[allow(dead_code)]
#[derive(Default)]
pub struct ControlledDelay {
    pub started: tokio::sync::Notify,
    pub release: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl ironflow::nodes::Node for ControlledDelay {
    fn node_type(&self) -> &str {
        "delay"
    }

    fn description(&self) -> &str {
        "Test-controlled scheduler task"
    }

    async fn execute(
        &self,
        _config: &serde_json::Value,
        _ctx: &ironflow::engine::types::Context,
    ) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
        self.started.notify_one();
        self.release.notified().await;
        Ok(ironflow::engine::types::NodeOutput::new())
    }
}

#[allow(dead_code)]
pub fn build_controlled_executor(flows_dir: &Path) -> (TestScheduler, Arc<ControlledDelay>) {
    let delay = Arc::new(ControlledDelay::default());
    let mut registry = NodeRegistry::with_builtins();
    registry.register(delay.clone());
    (build_executor_with_registry(flows_dir, registry), delay)
}
