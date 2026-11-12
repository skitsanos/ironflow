//! IF-211: user Lua run by a step (a function step, a `foreach` transform,
//! the top-level code of a child flow) is tracked work. A timed-out or
//! cancelled step stays open, with its handler unstarted, until the Lua call
//! has physically returned.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::*;
use crate::engine::types::{RunStatus, TaskStatus};
use crate::lua::runtime::LuaRuntime;
use serde_json::json;

/// How long the uninterruptible call must take at least.
const STUCK_FOR: Duration = Duration::from_millis(1200);
/// Observation window: past the 0.2 s step deadline, inside the stuck call.
const EARLY: Duration = Duration::from_millis(600);

/// A Lua statement that spends at least [`STUCK_FOR`] inside one C library
/// call, where the instruction hook cannot run. The cost of an anchored
/// pattern with three lazy repeats grows with the cube of the subject length,
/// so the length is measured once instead of assumed for the build profile.
async fn stuck_statement() -> String {
    static LENGTH: tokio::sync::OnceCell<usize> = tokio::sync::OnceCell::const_new();
    let length = LENGTH
        .get_or_init(|| async {
            tokio::task::spawn_blocking(|| {
                let lua = mlua::Lua::new();
                let mut length = 200_usize;
                loop {
                    let started = Instant::now();
                    lua.load(statement(length)).exec().unwrap();
                    let took = started.elapsed();
                    if took >= STUCK_FOR {
                        return length;
                    }
                    let scale = (1.5 * STUCK_FOR.as_secs_f64() / took.as_secs_f64().max(1e-4))
                        .cbrt()
                        .clamp(1.1, 3.0);
                    length = (length as f64 * scale).ceil() as usize;
                }
            })
            .await
            .unwrap()
        })
        .await;
    statement(*length)
}

fn statement(length: usize) -> String {
    format!("string.find(string.rep('a', {length}), '^.-.-.-b')")
}

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
            flow:step("seed", function(ctx) return {{ items = {{ 1 }} }} end)
            flow:step("call", {call}):depends_on("seed"){timeout}:on_error("handler")
            flow:step("handler", nodes.if211_handler({{}}))
            return flow
            "#
        );
        LuaRuntime::load_flow_from_string(&source, &self.registry).unwrap()
    }

    fn ctx(&self) -> Context {
        Context::from([(
            "_flow_dir".to_string(),
            json!(self.dir.path().to_string_lossy()),
        )])
    }
}

/// Every way a step runs user Lua, each stuck in one uninterruptible call.
async fn stuck_calls(fixture: &Fixture) -> Vec<(&'static str, String)> {
    let stuck = stuck_statement().await;
    fixture.write(
        "function_child.lua",
        &format!(
            r#"
            local flow = Flow.new("function-child")
            flow:step("spin", function(ctx) {stuck} return {{ spun = true }} end)
            return flow
            "#
        ),
    );
    fixture.write(
        "loading_child.lua",
        &format!(
            r#"
            {stuck}
            local flow = Flow.new("loading-child")
            flow:step("noop", function(ctx) return {{}} end)
            return flow
            "#
        ),
    );
    vec![
        (
            "function step",
            format!("function(ctx) {stuck} return {{ spun = true }} end"),
        ),
        (
            "foreach transform",
            format!(
                r#"nodes.foreach({{ source_key = "items",
                    transform = function(item) {stuck} return item end }})"#
            ),
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

#[tokio::test]
async fn a_timed_out_step_starts_no_handler_until_its_lua_call_returns() {
    let fixture = Fixture::new();
    for (case, call) in stuck_calls(&fixture).await {
        fixture.handler_started.store(false, Ordering::SeqCst);
        let flow = fixture.flow(&call, Some("0.2"));
        let handle = fixture.engine.start(&flow, fixture.ctx()).await.unwrap();
        let run_id = handle.id().to_string();
        let mut finished = Box::pin(handle.wait());

        call_is_running(&fixture, &run_id).await;
        let early = tokio::time::timeout(EARLY, &mut finished).await;
        assert!(
            early.is_err(),
            "{case}: the run settled while its Lua call was running"
        );
        assert!(
            !fixture.handler_started.load(Ordering::SeqCst),
            "{case}: the handler started while the Lua call was running"
        );
        let info = fixture.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Running, "{case}");
        assert_eq!(info.tasks["call"].status, TaskStatus::Running, "{case}");

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
}

#[tokio::test]
async fn a_cancelled_run_is_not_finalized_until_its_lua_call_returns() {
    let fixture = Fixture::new();
    for (case, call) in stuck_calls(&fixture).await {
        let flow = fixture.flow(&call, None);
        let handle = fixture.engine.start(&flow, fixture.ctx()).await.unwrap();
        let run_id = handle.id().to_string();

        call_is_running(&fixture, &run_id).await;
        // Let the worker enter the stuck call before it is cancelled.
        tokio::time::sleep(Duration::from_millis(150)).await;
        let mut cancelled = Box::pin(handle.cancel());
        let early = tokio::time::timeout(EARLY, &mut cancelled).await;
        assert!(
            early.is_err(),
            "{case}: cancellation returned while the Lua call was running"
        );
        let info = fixture.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Running, "{case}");

        tokio::time::timeout(Duration::from_secs(60), cancelled)
            .await
            .unwrap_or_else(|_| panic!("{case}: the run did not settle"))
            .unwrap();
        assert!(!fixture.handler_started.load(Ordering::SeqCst), "{case}");
        let info = fixture.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Cancelled, "{case}");
    }
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
