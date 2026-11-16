//! IF-188: a step that starts a child run stays open, with its capacity
//! held and its handler unstarted, until the child's tracked workers stop.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use super::*;
use crate::engine::types::{RetryConfig, RunStatus, StepDefinition, TaskStatus};
use crate::util::execution::run_tracked_blocking_step;
use serde_json::json;

/// Holds every worker of one test until released. Released on drop as well,
/// so a failed assertion cannot leave a blocking thread behind.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    opened: Condvar,
    started: AtomicUsize,
    stopped: AtomicUsize,
}

impl Gate {
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.opened.notify_all();
    }

    async fn worker_started(&self) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while self.started.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the child never reached its tracked worker");
    }
}

struct ReleaseOnDrop(Arc<Gate>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

/// A tracked worker that ignores cancellation and its deadline.
struct GatedWorkerNode(Arc<Gate>);

#[async_trait]
impl crate::nodes::Node for GatedWorkerNode {
    fn node_type(&self) -> &str {
        "if188_gated_worker"
    }
    fn description(&self) -> &str {
        "Block a tracked worker until the test releases it"
    }
    async fn execute(&self, _: &serde_json::Value, _: &Context) -> Result<Context> {
        let gate = self.0.clone();
        run_tracked_blocking_step(move |_execution| {
            gate.started.fetch_add(1, Ordering::SeqCst);
            let mut open = gate.open.lock().unwrap();
            while !*open {
                open = gate.opened.wait(open).unwrap();
            }
            gate.stopped.fetch_add(1, Ordering::SeqCst);
            Ok(Context::new())
        })
        .await
    }
}

struct HandlerNode(Arc<AtomicBool>);

#[async_trait]
impl crate::nodes::Node for HandlerNode {
    fn node_type(&self) -> &str {
        "if188_handler"
    }
    fn description(&self) -> &str {
        "Record that the recovery handler started"
    }
    async fn execute(&self, _: &serde_json::Value, _: &Context) -> Result<Context> {
        self.0.store(true, Ordering::SeqCst);
        Ok(Context::new())
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    _release: ReleaseOnDrop,
    gate: Arc<Gate>,
    handler_started: Arc<AtomicBool>,
    store: Arc<NullStateStore>,
    engine: WorkflowEngine,
    ctx: Context,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.lua"),
        r#"
        local flow = Flow.new("gated-child")
        flow:step("work", nodes.if188_gated_worker({}))
        return flow
        "#,
    )
    .unwrap();

    let gate = Arc::new(Gate::default());
    let handler_started = Arc::new(AtomicBool::new(false));
    let mut registry = NodeRegistry::with_builtins();
    registry.register(Arc::new(GatedWorkerNode(gate.clone())));
    registry.register(Arc::new(HandlerNode(handler_started.clone())));
    // Child registries are built from the base snapshot, so the test nodes
    // must be part of it.
    let base = Arc::new(registry.snapshot());
    crate::nodes::composition::register_nested(&mut registry, base);

    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), None);
    let ctx = Context::from([
        ("_flow_dir".to_string(), json!(dir.path().to_string_lossy())),
        (
            "calls".to_string(),
            json!([{"id": "call-1", "type": "function", "name": "gated", "arguments": {}}]),
        ),
    ]);
    Fixture {
        _dir: dir,
        _release: ReleaseOnDrop(gate.clone()),
        gate,
        handler_started,
        store,
        engine,
        ctx,
    }
}

/// Every composition node that waits for its child.
fn waiting_nodes() -> [(&'static str, serde_json::Value); 4] {
    [
        ("subworkflow", json!({"flow": "child.lua"})),
        (
            "parallel_subworkflows",
            json!({"flows": [{"flow": "child.lua"}]}),
        ),
        (
            "repeat_subworkflow",
            json!({"flow": "child.lua", "max_iterations": 1}),
        ),
        (
            "tool_dispatch",
            json!({"source_key": "calls", "tools": {"gated": {"flow": "child.lua"}}}),
        ),
    ]
}

fn parent_flow(
    node_type: &str,
    config: serde_json::Value,
    timeout_s: Option<f64>,
) -> FlowDefinition {
    let step = |name: &str, node_type: &str, config| StepDefinition {
        name: name.to_string(),
        node_type: node_type.to_string(),
        config,
        dependencies: Vec::new(),
        retry: RetryConfig::default(),
        timeout_s: None,
        route: None,
        on_error: None,
    };
    let mut call = step("call", node_type, config);
    call.timeout_s = timeout_s;
    call.on_error = Some("handler".to_string());
    FlowDefinition {
        name: format!("parent-{node_type}"),
        steps: vec![call, step("handler", "if188_handler", json!({}))],
    }
}

#[tokio::test]
async fn a_timed_out_step_starts_no_handler_until_its_child_worker_stops() {
    for (node_type, config) in waiting_nodes() {
        let fixture = fixture();
        let flow = parent_flow(node_type, config, Some(0.2));
        let handle = fixture
            .engine
            .start(&flow, fixture.ctx.clone())
            .await
            .unwrap();
        let run_id = handle.id().to_string();
        let mut finished = Box::pin(handle.wait());

        fixture.gate.worker_started().await;
        // The step deadline (0.2 s) passes inside this window.
        let early = tokio::time::timeout(Duration::from_millis(700), &mut finished).await;
        assert!(
            early.is_err(),
            "{node_type}: the run settled while its child worker was running"
        );
        assert!(
            !fixture.handler_started.load(Ordering::SeqCst),
            "{node_type}: the handler started while the child worker was running"
        );
        assert_eq!(fixture.gate.stopped.load(Ordering::SeqCst), 0);
        let info = fixture.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Running, "{node_type}");
        assert_eq!(
            info.tasks["call"].status,
            TaskStatus::Running,
            "{node_type}"
        );

        fixture.gate.release();
        tokio::time::timeout(Duration::from_secs(10), finished)
            .await
            .unwrap_or_else(|_| panic!("{node_type}: the run did not settle after release"))
            .unwrap();

        assert_eq!(
            fixture.gate.stopped.load(Ordering::SeqCst),
            1,
            "{node_type}"
        );
        assert!(
            fixture.handler_started.load(Ordering::SeqCst),
            "{node_type}"
        );
        let info = fixture.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Success, "{node_type}");
        assert_eq!(
            info.tasks["call"].error.as_deref(),
            Some("Task 'call' timed out after 0.2s total"),
            "{node_type}"
        );
    }
}

#[tokio::test]
async fn a_cancelled_run_is_not_finalized_until_its_child_worker_stops() {
    for (node_type, config) in waiting_nodes() {
        let fixture = fixture();
        let flow = parent_flow(node_type, config, None);
        let handle = fixture
            .engine
            .start(&flow, fixture.ctx.clone())
            .await
            .unwrap();
        let run_id = handle.id().to_string();

        fixture.gate.worker_started().await;
        let mut cancelled = Box::pin(handle.cancel());
        let early = tokio::time::timeout(Duration::from_millis(500), &mut cancelled).await;
        assert!(
            early.is_err(),
            "{node_type}: cancellation returned while the child worker was running"
        );
        assert_eq!(fixture.gate.stopped.load(Ordering::SeqCst), 0);
        let info = fixture.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Running, "{node_type}");

        fixture.gate.release();
        tokio::time::timeout(Duration::from_secs(10), cancelled)
            .await
            .unwrap_or_else(|_| panic!("{node_type}: the run did not settle after release"))
            .unwrap();

        assert_eq!(
            fixture.gate.stopped.load(Ordering::SeqCst),
            1,
            "{node_type}"
        );
        assert!(
            !fixture.handler_started.load(Ordering::SeqCst),
            "{node_type}"
        );
        let info = fixture.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Cancelled, "{node_type}");
    }
}

#[tokio::test]
async fn a_detached_child_does_not_hold_the_step_that_started_it() {
    let fixture = fixture();
    let flow = parent_flow(
        "subworkflow",
        json!({"flow": "child.lua", "wait": false}),
        Some(5.0),
    );

    let run_id = tokio::time::timeout(
        Duration::from_secs(5),
        fixture.engine.execute(&flow, fixture.ctx.clone()),
    )
    .await
    .expect("a detached child kept its parent step open")
    .unwrap();

    let info = fixture.store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.tasks["call"].status, TaskStatus::Success);
    fixture.gate.worker_started().await;
    assert_eq!(fixture.gate.stopped.load(Ordering::SeqCst), 0);
}
