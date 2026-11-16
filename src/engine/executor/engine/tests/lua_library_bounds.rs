//! IF-213: a Lua pattern that backtracks without bound used to hold its
//! step, its run and their admission for as long as the C matcher ran. It
//! now fails fast with the step-budget error, and a step timeout or a run
//! cancellation interrupts it inside the call.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::*;
use crate::engine::types::{RunStatus, TaskStatus};
use crate::lua::runtime::LuaRuntime;
use serde_json::json;

/// Never matches, and backtracks in time quartic in the subject length:
/// minutes in the reference matcher.
const PATHOLOGICAL: &str = r#"local found = string.find(string.rep("a", 1500), "^.-.-.-.-b")"#;
const BUDGET_ERROR: &str = "Lua pattern matching exceeded step budget of 50000000 \
                            (IRONFLOW_LUA_MAX_PATTERN_STEPS)";
/// Generous for a debug build; the reference matcher needs minutes.
const FAIL_FAST: Duration = Duration::from_secs(60);

struct HandlerNode(Arc<AtomicBool>);

#[async_trait]
impl crate::nodes::Node for HandlerNode {
    fn node_type(&self) -> &str {
        "if213_bounds_handler"
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
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("function_child.lua"),
            format!(
                r#"
                local flow = Flow.new("function-child")
                flow:step("spin", function(ctx) {PATHOLOGICAL} return {{ found = found }} end)
                return flow
                "#
            ),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("loading_child.lua"),
            format!(
                r#"
                {PATHOLOGICAL}
                local flow = Flow.new("loading-child")
                flow:step("noop", nodes.log({{ message = "loaded" }}))
                return flow
                "#
            ),
        )
        .unwrap();
        Self {
            dir,
            handler_started,
            store: store.clone(),
            engine: WorkflowEngine::new(registry.clone(), store, None),
            registry,
        }
    }

    fn flow(&self, call: &str, modifiers: &str) -> FlowDefinition {
        let source = format!(
            r#"
            local flow = Flow.new("if213-bounds")
            flow:step("call", {call}){modifiers}
            flow:step("handler", nodes.if213_bounds_handler({{}}))
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

/// Every way a step runs user Lua, each with the pathological pattern.
fn pathological_calls() -> Vec<(&'static str, String)> {
    vec![
        (
            "function step",
            format!("function(ctx) {PATHOLOGICAL} return {{ found = found }} end"),
        ),
        (
            "foreach transform",
            format!(
                r#"nodes.foreach({{ source_key = "items",
                    transform = function(item) {PATHOLOGICAL} return item end }})"#
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

#[tokio::test]
async fn a_pathological_pattern_fails_its_step_with_the_budget_error() {
    let fixture = Fixture::new();
    for (case, call) in pathological_calls() {
        fixture.handler_started.store(false, Ordering::SeqCst);
        let flow = fixture.flow(&call, r#":on_error("handler")"#);
        let run_id = tokio::time::timeout(FAIL_FAST, fixture.engine.execute(&flow, fixture.ctx()))
            .await
            .unwrap_or_else(|_| panic!("{case}: the pattern held its step"))
            .unwrap();

        let info = fixture.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Success, "{case}");
        assert_eq!(info.tasks["call"].status, TaskStatus::Failed, "{case}");
        let error = info.tasks["call"].error.as_deref().unwrap_or_default();
        assert!(error.contains(BUDGET_ERROR), "{case}: {error}");
        assert!(fixture.handler_started.load(Ordering::SeqCst), "{case}");
    }
}

#[tokio::test]
async fn a_step_timeout_interrupts_a_pathological_pattern() {
    let fixture = Fixture::new();
    for (case, call) in pathological_calls() {
        fixture.handler_started.store(false, Ordering::SeqCst);
        let flow = fixture.flow(&call, r#":timeout(0.2):on_error("handler")"#);
        let started = Instant::now();
        let run_id = tokio::time::timeout(FAIL_FAST, fixture.engine.execute(&flow, fixture.ctx()))
            .await
            .unwrap_or_else(|_| panic!("{case}: the pattern outlived its step deadline"))
            .unwrap();
        // The matcher checks the deadline every few thousand steps.
        assert!(started.elapsed() < Duration::from_secs(5), "{case}");

        let info = fixture.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Success, "{case}");
        assert_eq!(
            info.tasks["call"].error.as_deref(),
            Some("Task 'call' timed out after 0.2s total"),
            "{case}"
        );
        assert!(fixture.handler_started.load(Ordering::SeqCst), "{case}");
    }
}

#[tokio::test]
async fn cancellation_interrupts_a_pathological_pattern() {
    let fixture = Fixture::new();
    for (case, call) in pathological_calls() {
        let flow = fixture.flow(&call, "");
        let handle = fixture.engine.start(&flow, fixture.ctx()).await.unwrap();
        let run_id = handle.id().to_string();
        tokio::time::sleep(Duration::from_millis(150)).await;

        let started = Instant::now();
        tokio::time::timeout(FAIL_FAST, handle.cancel())
            .await
            .unwrap_or_else(|_| panic!("{case}: the pattern outlived cancellation"))
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(5), "{case}");
        let info = fixture.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Cancelled, "{case}");
    }
}
