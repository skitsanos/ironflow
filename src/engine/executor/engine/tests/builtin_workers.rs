//! IF-213: the worker of a built-in blocking node is tracked work. A step
//! that times out or is cancelled stays open, with its handler unstarted and
//! the run unfinished, until the worker has physically stopped.
//!
//! Each node's worker is stalled after its operation by the test gate of the
//! blocking-step bridge, where no cancellation checkpoint can end it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::*;
use crate::engine::types::{RunStatus, TaskStatus};
use crate::lua::runtime::LuaRuntime;
use crate::util::execution::test_support::{ReleaseOnDrop, StallGate};
use serde_json::json;

/// Observation window: past the 0.2 s step deadline.
const EARLY: Duration = Duration::from_millis(600);

struct HandlerNode(Arc<AtomicBool>);

#[async_trait]
impl crate::nodes::Node for HandlerNode {
    fn node_type(&self) -> &str {
        "if213_handler"
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
        let registry = Arc::new(registry);
        let store = Arc::new(NullStateStore::new());
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("tree")).unwrap();
        std::fs::write(dir.path().join("tree/a.txt"), "alpha").unwrap();
        Self {
            dir,
            handler_started,
            store: store.clone(),
            engine: WorkflowEngine::new(registry.clone(), store, None),
            registry,
        }
    }

    fn flow(&self, call: &str, timeout: Option<&str>) -> FlowDefinition {
        let timeout = timeout.map_or(String::new(), |seconds| format!(":timeout({seconds})"));
        let source = format!(
            r#"
            local flow = Flow.new("if213")
            flow:step("call", {call}){timeout}:on_error("handler")
            flow:step("handler", nodes.if213_handler({{}}))
            return flow
            "#
        );
        LuaRuntime::load_flow_from_string(&source, &self.registry).unwrap()
    }

    /// The Lua expression of one family's node with its paths filled in.
    fn call(&self, template: &str) -> String {
        template
            .replace("{dir}", &self.dir.path().to_string_lossy())
            .replace(
                "{fixtures}",
                concat!(env!("CARGO_MANIFEST_DIR"), "/examples/fixtures"),
            )
    }

    fn ctx(&self) -> Context {
        Context::from([
            ("html".to_string(), json!("<h1>Title</h1><p>Body</p>")),
            ("payload".to_string(), json!("{\"name\":\"a\"}")),
            ("items".to_string(), json!([1])),
        ])
    }
}

/// One node per family that does its work on a blocking worker. `{dir}` and
/// `{fixtures}` are replaced with real paths, because not every path
/// parameter is interpolated.
const FAMILIES: &[(&str, &str)] = &[
    (
        "xml_parse",
        r#"nodes.xml_parse({ input = "<a><b>1</b></a>" })"#,
    ),
    (
        "json_validate",
        r#"nodes.json_validate({ source_key = "payload", schema = { type = "object" } })"#,
    ),
    (
        "cache_set (file backend)",
        r#"nodes.cache_set({ key = "k", value = "v", backend = "file",
            cache_dir = "{dir}/cache" })"#,
    ),
    (
        "html_to_markdown",
        r#"nodes.html_to_markdown({ source_key = "html" })"#,
    ),
    (
        "markdown_to_html",
        r##"nodes.markdown_to_html({ input = "# Title" })"##,
    ),
    (
        "extract_html",
        r#"nodes.extract_html({ path = "{dir}/page.html" })"#,
    ),
    (
        "extract_vtt",
        r#"nodes.extract_vtt({ path = "{fixtures}/ironflow-transcript.vtt" })"#,
    ),
    (
        "extract_word",
        r#"nodes.extract_word({ path = "{fixtures}/ironflow-sample.docx" })"#,
    ),
    (
        "extract_xlsx",
        r#"nodes.extract_xlsx({ path = "{fixtures}/ironflow-sample.xlsx" })"#,
    ),
    (
        "extract_pptx",
        r#"nodes.extract_pptx({ path = "{fixtures}/ironflow-sample.pptx" })"#,
    ),
    (
        "extract_pdf",
        r#"nodes.extract_pdf({ path = "{fixtures}/ironflow-sample.pdf" })"#,
    ),
    (
        "pdf_metadata",
        r#"nodes.pdf_metadata({ path = "{fixtures}/ironflow-sample.pdf" })"#,
    ),
    (
        "image_metadata",
        r#"nodes.image_metadata({ path = "{fixtures}/ironflow-sample.png" })"#,
    ),
    (
        "write_file",
        r#"nodes.write_file({ path = "{dir}/out.txt", content = "x" })"#,
    ),
    (
        "zip_create",
        r#"nodes.zip_create({ source = "{dir}/tree", zip_path = "{dir}/tree.zip" })"#,
    ),
    ("function step", "function(ctx) return { done = true } end"),
    (
        "foreach transform",
        r#"nodes.foreach({ source_key = "items",
            transform = function(item) return item end })"#,
    ),
];

fn gated<F>(test: impl FnOnce(Arc<StallGate>, Fixture) -> F)
where
    F: std::future::Future<Output = ()>,
{
    let gate = Arc::new(StallGate::default());
    let runtime = gate.runtime();
    // Declared after the runtime, so it is dropped first: a runtime waits
    // for its blocking threads when it is dropped.
    let _release = ReleaseOnDrop(gate.clone());
    let fixture = Fixture::new();
    std::fs::write(fixture.dir.path().join("page.html"), "<p>Body</p>").unwrap();
    runtime.block_on(test(gate.clone(), fixture));
}

#[test]
fn a_timed_out_step_starts_no_handler_until_its_worker_stops() {
    gated(|gate, fixture| async move {
        for (case, call) in FAMILIES {
            fixture.handler_started.store(false, Ordering::SeqCst);
            gate.arm();
            let flow = fixture.flow(&fixture.call(call), Some("0.2"));
            let handle = fixture.engine.start(&flow, fixture.ctx()).await.unwrap();
            let run_id = handle.id().to_string();
            let mut finished = Box::pin(handle.wait());

            tokio::select! {
                () = gate.worker_held() => {}
                outcome = &mut finished => panic!(
                    "{case}: the run settled without a blocking worker: {outcome:?} {:?}",
                    fixture.store.get_run_info(&run_id).await.unwrap().tasks["call"].error
                ),
            }
            let early = tokio::time::timeout(EARLY, &mut finished).await;
            assert!(
                early.is_err(),
                "{case}: the run settled while its worker was running"
            );
            assert!(
                !fixture.handler_started.load(Ordering::SeqCst),
                "{case}: the handler started while the worker was running"
            );
            let info = fixture.store.get_run_info(&run_id).await.unwrap();
            assert_eq!(info.status, RunStatus::Running, "{case}");
            assert_eq!(info.tasks["call"].status, TaskStatus::Running, "{case}");

            gate.release();
            tokio::time::timeout(Duration::from_secs(30), finished)
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
fn a_cancelled_run_is_not_finalized_until_its_worker_stops() {
    gated(|gate, fixture| async move {
        for (case, call) in FAMILIES {
            gate.arm();
            let flow = fixture.flow(&fixture.call(call), None);
            let handle = fixture.engine.start(&flow, fixture.ctx()).await.unwrap();
            let run_id = handle.id().to_string();

            gate.worker_held().await;
            let mut cancelled = Box::pin(handle.cancel());
            let early = tokio::time::timeout(EARLY, &mut cancelled).await;
            assert!(
                early.is_err(),
                "{case}: cancellation returned while the worker was running"
            );
            let info = fixture.store.get_run_info(&run_id).await.unwrap();
            assert_eq!(info.status, RunStatus::Running, "{case}");

            gate.release();
            tokio::time::timeout(Duration::from_secs(30), cancelled)
                .await
                .unwrap_or_else(|_| panic!("{case}: the run did not settle"))
                .unwrap();
            assert!(!fixture.handler_started.load(Ordering::SeqCst), "{case}");
            let info = fixture.store.get_run_info(&run_id).await.unwrap();
            assert_eq!(info.status, RunStatus::Cancelled, "{case}");
        }
    });
}

#[test]
fn a_step_that_finishes_in_time_is_unchanged() {
    gated(|_gate, fixture| async move {
        for (case, call) in FAMILIES {
            let flow = fixture.flow(&fixture.call(call), Some("30"));
            let run_id = fixture.engine.execute(&flow, fixture.ctx()).await.unwrap();
            let info = fixture.store.get_run_info(&run_id).await.unwrap();
            assert_eq!(info.status, RunStatus::Success, "{case}");
            assert_eq!(
                info.tasks["call"].status,
                TaskStatus::Success,
                "{case}: {:?}",
                info.tasks["call"].error
            );
            assert!(!fixture.handler_started.load(Ordering::SeqCst), "{case}");
        }
    });
}
