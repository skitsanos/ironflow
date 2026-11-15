//! IF-213: the steps of a child run observe the deadline of the parent step
//! that waits for it, so deadline-aware work in the child cannot plan past
//! the moment its parent gives up.

use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use super::*;
use crate::engine::types::RunStatus;
use crate::lua::runtime::LuaRuntime;
use crate::util::execution::current_execution_deadline;
use serde_json::json;

type Observed = Arc<Mutex<Vec<(String, Option<Duration>)>>>;

/// Records how far away the deadline visible to its step is.
struct ProbeNode(Observed);

#[async_trait]
impl crate::nodes::Node for ProbeNode {
    fn node_type(&self) -> &str {
        "if213_deadline_probe"
    }
    fn description(&self) -> &str {
        "Record the execution deadline visible to this step"
    }
    async fn execute(&self, config: &serde_json::Value, _: &Context) -> Result<Context> {
        let remaining = current_execution_deadline()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()));
        let label = config["label"].as_str().unwrap_or_default().to_string();
        self.0.lock().unwrap().push((label, remaining));
        Ok(Context::new())
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    observed: Observed,
    store: Arc<NullStateStore>,
    registry: Arc<NodeRegistry>,
    engine: WorkflowEngine,
}

impl Fixture {
    fn new() -> Self {
        let observed = Observed::default();
        let mut registry = NodeRegistry::with_builtins();
        registry.register(Arc::new(ProbeNode(observed.clone())));
        let base = Arc::new(registry.snapshot());
        crate::nodes::composition::register_nested(&mut registry, base);
        let registry = Arc::new(registry);
        let store = Arc::new(NullStateStore::new());
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("child.lua"),
            r#"
            local flow = Flow.new("child")
            flow:step("plain", nodes.if213_deadline_probe({ label = "plain" }))
            flow:step("short", nodes.if213_deadline_probe({ label = "short" })):timeout(5)
            flow:step("long", nodes.if213_deadline_probe({ label = "long" })):timeout(3600)
            flow:step("nested", nodes.subworkflow({ flow = "grandchild.lua" }))
            return flow
            "#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("grandchild.lua"),
            r#"
            local flow = Flow.new("grandchild")
            flow:step("deep", nodes.if213_deadline_probe({ label = "deep" }))
            flow:step("finish", function(ctx) return { done = true } end)
            return flow
            "#,
        )
        .unwrap();
        Self {
            dir,
            observed,
            store: store.clone(),
            engine: WorkflowEngine::new(registry.clone(), store, None),
            registry,
        }
    }

    /// Run a parent whose `call` step is `call` and return what each probe
    /// saw.
    async fn observe(&self, call: &str) -> std::collections::HashMap<String, Option<Duration>> {
        self.observed.lock().unwrap().clear();
        let source = format!(
            r#"
            local flow = Flow.new("parent")
            flow:step("call", {call})
            return flow
            "#
        );
        let flow = LuaRuntime::load_flow_from_string(&source, &self.registry).unwrap();
        let ctx = Context::from([(
            "_flow_dir".to_string(),
            json!(self.dir.path().to_string_lossy()),
        )]);
        let run_id = self.engine.execute(&flow, ctx).await.unwrap();
        let info = self.store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Success, "{call}");
        self.observed.lock().unwrap().iter().cloned().collect()
    }
}

fn within(remaining: Option<Duration>, seconds: u64) -> bool {
    remaining.is_some_and(|remaining| {
        remaining <= Duration::from_secs(seconds)
            && remaining > Duration::from_secs(seconds.saturating_sub(4))
    })
}

#[tokio::test]
async fn child_steps_observe_the_waiting_parent_step_deadline() {
    let fixture = Fixture::new();
    let seen = fixture
        .observe(r#"nodes.subworkflow({ flow = "child.lua" })):timeout(60"#)
        .await;

    // No timeout of its own: the parent step's deadline.
    assert!(within(seen["plain"], 60), "{seen:?}");
    // An earlier deadline of its own wins.
    assert!(within(seen["short"], 5), "{seen:?}");
    // A later one does not extend past the parent step.
    assert!(within(seen["long"], 60), "{seen:?}");
    // Every level below inherits it.
    assert!(within(seen["deep"], 60), "{seen:?}");
}

#[tokio::test]
async fn every_waiting_composition_node_passes_its_deadline_down() {
    let fixture = Fixture::new();
    for call in [
        r#"nodes.parallel_subworkflows({ flows = { { flow = "grandchild.lua" } } })):timeout(60"#,
        r#"nodes.repeat_subworkflow({ flow = "grandchild.lua", max_iterations = 1,
            until_key = "done" })):timeout(60"#,
    ] {
        let seen = fixture.observe(call).await;
        assert!(within(seen["deep"], 60), "{call}: {seen:?}");
    }
}

#[tokio::test]
async fn a_parent_step_without_a_timeout_adds_no_deadline() {
    let fixture = Fixture::new();
    let seen = fixture
        .observe(r#"nodes.subworkflow({ flow = "child.lua" })"#)
        .await;

    assert_eq!(seen["plain"], None, "{seen:?}");
    assert!(within(seen["short"], 5), "{seen:?}");
    assert!(within(seen["long"], 3600), "{seen:?}");
    assert_eq!(seen["deep"], None, "{seen:?}");
}
