//! IF-211: user Lua run by a step (a function step, a `foreach` transform,
//! the top-level code of a child flow) is tracked work. A timed-out or
//! cancelled step stays open, with its handler unstarted, until the worker
//! that runs the Lua has physically returned.
//!
//! Since IF-213 no Lua library call runs long enough to stand in for a stuck
//! worker, so the worker is held after its Lua returned by the test gate of
//! the blocking-step bridge.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::*;
use crate::engine::types::{RunStatus, TaskStatus};
use crate::lua::runtime::LuaRuntime;
use crate::util::execution::test_support::{ReleaseOnDrop, StallGate};
use serde_json::json;

/// Observation window: past the 0.2 s step deadline, with the worker held.
const EARLY: Duration = Duration::from_millis(600);

struct HandlerNode(Arc<AtomicBool>);

#[async_trait]
impl crate::nodes::Node for HandlerNode {
    fn node_type(&self) -> &str {
        "if211_handler"
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
    dir: tempfile::TempDir,
    handler_started: Arc<AtomicBool>,
    store: Arc<NullStateStore>,
    registry: Arc<NodeRegistry>,
    engine: WorkflowEngine,
}

impl Fixture {
    fn new() -> Self {
        let handler_started = Arc::new(AtomicBool::new(false));
        let mut registry = NodeRegistry::with_builtins();
        registry.register(Arc::new(HandlerNode(handler_started.clone())));
        let base = Arc::new(registry.snapshot());
        crate::nodes::composition::register_nested(&mut registry, base);
        let registry = Arc::new(registry);
        let store = Arc::new(NullStateStore::new());
        Self {
            dir: tempfile::tempdir().unwrap(),
            handler_started,
            store: store.clone(),
            engine: WorkflowEngine::new(registry.clone(), store, None),
            registry,
        }
    }

    fn write(&self, name: &str, source: &str) {
        std::fs::write(self.dir.path().join(name), source).unwrap();
    }

    /// A flow whose `call` step is `call`, with an optional timeout and the
    /// recording handler.
    fn flow(&self, call: &str, timeout: Option<&str>) -> FlowDefinition {
        let timeout = timeout.map_or(String::new(), |seconds| format!(":timeout({seconds})"));
        let source = format!(
            r#"
            local flow = Flow.new("if211")
            flow:step("call", {call}){timeout}:on_error("handler")
            flow:step("handler", nodes.if211_handler({{}}))
            return flow
            "#
        );
        LuaRuntime::load_flow_from_string(&source, &self.registry).unwrap()
    }

    fn ctx(&self) -> Context {
        Context::from([
            (
                "_flow_dir".to_string(),
                json!(self.dir.path().to_string_lossy()),
            ),
            ("items".to_string(), json!([1])),
        ])
    }
}

/// Every way a step runs user Lua.
fn lua_calls(fixture: &Fixture) -> Vec<(&'static str, String)> {
    fixture.write(
        "function_child.lua",
        r#"
        local flow = Flow.new("function-child")
        flow:step("spin", function(ctx) return { spun = true } end)
        return flow
        "#,
    );
    fixture.write(
        "loading_child.lua",
        r#"
        local loaded = string.rep("a", 10)
        local flow = Flow.new("loading-child")
        flow:step("noop", nodes.log({ message = "loaded" }))
        return flow
        "#,
    );
    vec![
        (
            "function step",
            "function(ctx) return { spun = true } end".to_string(),
        ),
        (
            "foreach transform",
            r#"nodes.foreach({ source_key = "items",
                transform = function(item) return item end })"#
                .to_string(),
        ),
        (
            "function step in a child flow",
            r#"nodes.subworkflow({ flow = "function_child.lua" })"#.to_string(),
        ),
        (
            "top-level code of a child flow",
            r#"nodes.subworkflow({ flow = "loading_child.lua" })"#.to_string(),
        ),
    ]
}

async fn call_is_running(fixture: &Fixture, run_id: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let info = fixture.store.get_run_info(run_id).await.unwrap();
            if info
                .tasks
                .get("call")
                .is_some_and(|task| task.status == TaskStatus::Running)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the call step never started");
}

/// Run `test` on a runtime whose blocking-step workers the gate can hold.
fn gated<F>(test: impl FnOnce(Arc<StallGate>, Fixture) -> F)
where
    F: std::future::Future<Output = ()>,
{
    let gate = Arc::new(StallGate::default());
    let runtime = gate.runtime();
    // Declared after the runtime, so it is dropped first: a runtime waits
    // for its blocking threads when it is dropped.
    let _release = ReleaseOnDrop(gate.clone());
    runtime.block_on(test(gate.clone(), Fixture::new()));
}

#[test]
fn a_timed_out_step_starts_no_handler_until_its_lua_worker_returns() {
    gated(|gate, fixture| async move {
        for (case, call) in lua_calls(&fixture) {
            fixture.handler_started.store(false, Ordering::SeqCst);
            gate.arm();
            let flow = fixture.flow(&call, Some("0.2"));
            let handle = fixture.engine.start(&flow, fixture.ctx()).await.unwrap();
            let run_id = handle.id().to_string();
            let mut finished = Box::pin(handle.wait());

            gate.worker_held().await;
            let early = tokio::time::timeout(EARLY, &mut finished).await;
            assert!(
                early.is_err(),
                "{case}: the run settled while its Lua worker was running"
            );
            assert!(
                !fixture.handler_started.load(Ordering::SeqCst),
                "{case}: the handler started while the Lua worker was running"
            );
            let info = fixture.store.get_run_info(&run_id).await.unwrap();
            assert_eq!(info.status, RunStatus::Running, "{case}");
            assert_eq!(info.tasks["call"].status, TaskStatus::Running, "{case}");

            gate.release();
            tokio::time::timeout(Duration::from_secs(60), finished)
                .await
                .unwrap_or_else(|_| panic!("{case}: the run did not settle"))
                .unwrap();
            assert!(fixture.handler_started.load(Ordering::SeqCst), "{case}");
            let info = fixture.store.get_run_info(&run_id).await.unwrap();
            assert_eq!(info.status, RunStatus::Success, "{case}");
            assert_eq!(
                info.tasks["call"].error.as_deref(),
                Some("Task 'call' timed out after 0.2s total"),
                "{case}"
            );
        }
    });
}

#[test]
fn a_cancelled_run_is_not_finalized_until_its_lua_worker_returns() {
    gated(|gate, fixture| async move {
        for (case, call) in lua_calls(&fixture) {
            gate.arm();
            let flow = fixture.flow(&call, None);
            let handle = fixture.engine.start(&flow, fixture.ctx()).await.unwrap();
            let run_id = handle.id().to_string();

            gate.worker_held().await;
            let mut cancelled = Box::pin(handle.cancel());
            let early = tokio::time::timeout(EARLY, &mut cancelled).await;
            assert!(
                early.is_err(),
                "{case}: cancellation returned while the Lua worker was running"
            );
            let info = fixture.store.get_run_info(&run_id).await.unwrap();
            assert_eq!(info.status, RunStatus::Running, "{case}");

            gate.release();
            tokio::time::timeout(Duration::from_secs(60), cancelled)
                .await
                .unwrap_or_else(|_| panic!("{case}: the run did not settle"))
                .unwrap();
            assert!(!fixture.handler_started.load(Ordering::SeqCst), "{case}");
            let info = fixture.store.get_run_info(&run_id).await.unwrap();
            assert_eq!(info.status, RunStatus::Cancelled, "{case}");
        }
    });
}

#[tokio::test]
async fn a_lua_loop_is_stopped_at_the_deadline_and_on_cancellation() {
    let fixture = Fixture::new();
    // Few instructions per iteration, so the step deadline is reached long
    // before the instruction budget.
    let spin = "function(ctx) while true do string.rep('a', 100000) end end";

    let run_id = tokio::time::timeout(
        Duration::from_secs(10),
        fixture
            .engine
            .execute(&fixture.flow(spin, Some("0.2")), fixture.ctx()),
    )
    .await
    .expect("a Lua loop outlived its step deadline")
    .unwrap();
    assert!(fixture.handler_started.load(Ordering::SeqCst));
    let info = fixture.store.get_run_info(&run_id).await.unwrap();
    assert_eq!(
        info.tasks["call"].error.as_deref(),
        Some("Task 'call' timed out after 0.2s total")
    );

    let handle = fixture
        .engine
        .start(&fixture.flow(spin, None), fixture.ctx())
        .await
        .unwrap();
    let run_id = handle.id().to_string();
    call_is_running(&fixture, &run_id).await;
    tokio::time::timeout(Duration::from_secs(10), handle.cancel())
        .await
        .expect("a Lua loop outlived cancellation")
        .unwrap();
    let info = fixture.store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Cancelled);
}

#[tokio::test]
async fn a_function_step_that_finishes_in_time_is_unchanged() {
    let fixture = Fixture::new();
    let flow = fixture.flow(
        "function(ctx) return { doubled = ctx.items[1] * 2 } end",
        Some("30"),
    );
    let started = Instant::now();
    let run_id = fixture.engine.execute(&flow, fixture.ctx()).await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));

    assert!(!fixture.handler_started.load(Ordering::SeqCst));
    let info = fixture.store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks["call"].status, TaskStatus::Success);
    assert_eq!(info.ctx["doubled"], json!(2));
}
