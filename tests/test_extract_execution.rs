//! Prove each synchronous extractor participates in cooperative deadlines.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::Context;
use ironflow::engine::types::{FlowDefinition, RetryConfig, RunStatus, StepDefinition, TaskStatus};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::util::execution::with_execution_deadline;
use tokio::sync::oneshot;
use tracing_subscriber::prelude::*;

const INPUT_TRACE_TARGET: &str = "ironflow::extract::input";
const CANCELLATION_TRACE_TARGET: &str = "ironflow::execution::cooperative_worker";

#[derive(Clone)]
struct ExtractorWorkerGate {
    state: Arc<GateState>,
}

struct GateState {
    block_input: AtomicBool,
    started: Mutex<Option<oneshot::Sender<()>>>,
    cancellation_requested: Mutex<Option<oneshot::Sender<()>>>,
    released: Mutex<bool>,
    released_cv: Condvar,
}

struct ReleaseWorkerOnDrop(ExtractorWorkerGate);

impl Drop for ReleaseWorkerOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

impl ExtractorWorkerGate {
    fn new() -> (Self, oneshot::Receiver<()>, oneshot::Receiver<()>) {
        let (started_tx, started_rx) = oneshot::channel();
        let (cancelled_tx, cancelled_rx) = oneshot::channel();
        (
            Self {
                state: Arc::new(GateState {
                    block_input: AtomicBool::new(true),
                    started: Mutex::new(Some(started_tx)),
                    cancellation_requested: Mutex::new(Some(cancelled_tx)),
                    released: Mutex::new(false),
                    released_cv: Condvar::new(),
                }),
            },
            started_rx,
            cancelled_rx,
        )
    }

    fn release(&self) {
        *self.state.released.lock().unwrap() = true;
        self.state.released_cv.notify_all();
    }
}

impl<S> tracing_subscriber::Layer<S> for ExtractorWorkerGate
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        match event.metadata().target() {
            INPUT_TRACE_TARGET if self.state.block_input.swap(false, Ordering::AcqRel) => {
                if let Some(started) = self.state.started.lock().unwrap().take() {
                    let _ = started.send(());
                }
                let mut released = self.state.released.lock().unwrap();
                while !*released {
                    released = self.state.released_cv.wait(released).unwrap();
                }
            }
            CANCELLATION_TRACE_TARGET => {
                if let Some(cancelled) = self.state.cancellation_requested.lock().unwrap().take() {
                    let _ = cancelled.send(());
                }
            }
            _ => {}
        }
    }
}

#[tokio::test]
async fn extractors_observe_an_expired_deadline_before_blocking_work() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/fixtures");
    let directory = tempfile::tempdir().unwrap();
    let html = directory.path().join("sample.html");
    std::fs::write(&html, "<p>bounded</p>").unwrap();

    let cases = [
        ("extract_html", html),
        ("extract_pdf", fixtures.join("ironflow-sample.pdf")),
        ("extract_word", fixtures.join("ironflow-sample.docx")),
        ("extract_pptx", fixtures.join("ironflow-sample.pptx")),
        ("extract_srt", fixtures.join("ironflow-transcript.srt")),
        ("extract_vtt", fixtures.join("ironflow-transcript.vtt")),
        ("extract_xlsx", fixtures.join("ironflow-sample.xlsx")),
    ];
    let registry = NodeRegistry::with_builtins();
    for (node_name, path) in cases {
        let node = registry.get(node_name).unwrap();
        let config = serde_json::json!({ "path": path });
        let error = with_execution_deadline(
            Some(tokio::time::Instant::now()),
            node.execute(&config, &Context::new()),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("step deadline exceeded"),
            "{node_name}: {error}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn extractor_cancellation_retains_run_admission_until_the_worker_drains() {
    let (gate, started, cancellation_requested) = ExtractorWorkerGate::new();
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(gate.clone()))
        .expect("the focused extraction test owns its process-wide subscriber");
    let _release_worker_on_drop = ReleaseWorkerOnDrop(gate.clone());

    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("controlled.vtt");
    std::fs::write(
        &input,
        "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\ncontrolled cue\n",
    )
    .unwrap();

    let store = Arc::new(JsonStateStore::new(directory.path().join("state")));
    let engine = WorkflowEngine::new(
        Arc::new(NodeRegistry::with_builtins()),
        store.clone(),
        Some(1),
    );
    let flow = FlowDefinition {
        name: "controlled-extractor-cancellation".to_string(),
        steps: vec![StepDefinition {
            name: "extract".to_string(),
            node_type: "extract_vtt".to_string(),
            config: serde_json::json!({ "path": input }),
            dependencies: Vec::new(),
            retry: RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        }],
    };

    // Model the API's run-admission ownership: it is released only when the
    // RunHandle settles, which must remain coupled to the physical worker.
    let admission = Arc::new(tokio::sync::Semaphore::new(1));
    let permit = admission.clone().acquire_owned().await.unwrap();
    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();

    tokio::time::timeout(std::time::Duration::from_secs(2), started)
        .await
        .expect("the real extractor never began reading")
        .unwrap();
    let cancellation = tokio::spawn(async move {
        let _permit = permit;
        handle.cancel().await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), cancellation_requested)
        .await
        .expect("the extractor waiter did not receive cancellation")
        .unwrap();

    assert!(
        !cancellation.is_finished(),
        "the run settled while its physical extraction worker was gated"
    );
    assert!(
        admission.try_acquire().is_err(),
        "run admission was released before the extraction worker drained"
    );

    gate.release();
    let completed_run = tokio::time::timeout(std::time::Duration::from_secs(2), cancellation)
        .await
        .expect("the cooperative extraction worker did not drain")
        .unwrap()
        .unwrap();
    assert_eq!(completed_run, run_id);
    let _released = admission
        .try_acquire()
        .expect("run admission remained held after the worker drained");

    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Cancelled);
    assert_eq!(info.tasks["extract"].status, TaskStatus::Cancelled);
}
