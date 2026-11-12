//! IF-181: a context or step output nested deeper than the stores can read
//! back is refused as a request or step error, never written as a run record
//! that nothing can read.

use std::sync::Arc;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{
    Context, FlowDefinition, RetryConfig, RunInfo, RunStatus, StepDefinition, TaskStatus,
};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::storage::sql_store::SqlStateStore;

#[path = "support/json_depth.rs"]
mod depth_support;

use depth_support::{ADMITTED_DEPTH, REPORTED_DEPTH, nested_text, nested_value};

const LIMIT_MESSAGE: &str = "nested deeper than 123 levels";

struct Store {
    label: &'static str,
    store: Arc<dyn StateStore>,
    directory: tempfile::TempDir,
}

async fn stores() -> Vec<Store> {
    let json = tempfile::tempdir().unwrap();
    let sqlite = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        sqlite.path().join("state.sqlite").display()
    );
    vec![
        Store {
            label: "json",
            store: Arc::new(JsonStateStore::new(json.path())),
            directory: json,
        },
        Store {
            label: "sqlite",
            store: Arc::new(SqlStateStore::new(&url).await.unwrap()),
            directory: sqlite,
        },
    ]
}

fn flow(node_type: &str) -> FlowDefinition {
    FlowDefinition {
        name: "depth".to_string(),
        steps: vec![StepDefinition {
            name: "parse".to_string(),
            node_type: node_type.to_string(),
            config: serde_json::json!({ "source_key": "raw", "output_key": "parsed" }),
            dependencies: Vec::new(),
            // A refused output is final: retrying would only repeat the node.
            retry: RetryConfig {
                max_retries: 2,
                backoff_s: 0.0,
            },
            timeout_s: None,
            route: None,
            on_error: None,
        }],
    }
}

async fn run(store: &Store, node_type: &str, raw: String) -> RunInfo {
    let engine = WorkflowEngine::new(
        Arc::new(NodeRegistry::with_builtins()),
        store.store.clone(),
        None,
    );
    let context = Context::from([("raw".to_string(), serde_json::Value::String(raw))]);
    let run_id = engine
        .execute(&flow(node_type), context)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "{}: the run must finish with a readable record: {error:#}",
                store.label
            )
        });
    store
        .store
        .get_run_info(&run_id)
        .await
        .unwrap_or_else(|error| panic!("{}: the run must read back: {error}", store.label))
}

fn assert_step_refused(store: &Store, info: &RunInfo) {
    let label = store.label;
    assert_eq!(info.status, RunStatus::Failed, "{label}");
    let task = &info.tasks["parse"];
    assert_eq!(task.status, TaskStatus::Failed, "{label}");
    assert_eq!(
        task.attempt, 1,
        "{label}: a refused output must not be retried"
    );
    let error = task.error.as_deref().unwrap_or_default();
    assert!(error.contains(LIMIT_MESSAGE), "{label}: {error}");
    assert!(
        !info.ctx.contains_key("parsed"),
        "{label}: refused output reached the context"
    );
    assert!(
        std::fs::read_dir(store.directory.path().join(".ironflow-run-leases-v1"))
            .map(|entries| entries
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().ends_with(".lease")))
            .unwrap_or(true),
        "{label}: a finished run must not keep its ownership lease"
    );
}

#[tokio::test]
async fn json_parse_output_deeper_than_the_limit_fails_its_step() {
    for store in stores().await {
        // 126 is the depth reported in IF-181; 127 is the deepest text the
        // parser admits and was unreadable on the SQL stores as well.
        for depth in [ADMITTED_DEPTH + 1, REPORTED_DEPTH, 127] {
            let info = run(&store, "json_parse", nested_text(depth)).await;
            assert_step_refused(&store, &info);
        }
    }
}

#[tokio::test]
async fn json_parse_output_at_the_limit_is_stored_and_read_back() {
    for store in stores().await {
        let info = run(&store, "json_parse", nested_text(ADMITTED_DEPTH)).await;
        assert_eq!(info.status, RunStatus::Success, "{}", store.label);
        assert!(
            info.ctx["parsed"] == nested_value(ADMITTED_DEPTH),
            "{}: admitted output must read back unchanged",
            store.label
        );
        assert!(
            info.tasks["parse"].output
                == Some(serde_json::json!({
                    "parsed": nested_value(ADMITTED_DEPTH)
                })),
            "{}: admitted task output must read back unchanged",
            store.label
        );
    }
}

#[tokio::test]
async fn xml_parse_output_deeper_than_the_limit_fails_its_step() {
    // `xml_parse` admits 128 nested elements (IF-037); each becomes one object.
    let xml = format!("{}{}", "<e>".repeat(128), "</e>".repeat(128));
    for store in stores().await {
        let info = run(&store, "xml_parse", xml.clone()).await;
        assert_step_refused(&store, &info);
    }
}

#[tokio::test]
async fn a_run_cannot_start_with_a_context_deeper_than_the_limit() {
    for store in stores().await {
        let engine = WorkflowEngine::new(
            Arc::new(NodeRegistry::with_builtins()),
            store.store.clone(),
            None,
        );
        for depth in [ADMITTED_DEPTH + 1, REPORTED_DEPTH] {
            let context = Context::from([("a".to_string(), nested_value(depth))]);
            let error = match engine.start(&flow("json_parse"), context).await {
                Ok(handle) => panic!("{}: run {} was started", store.label, handle.id()),
                Err(error) => format!("{error:#}"),
            };
            assert!(error.contains(LIMIT_MESSAGE), "{}: {error}", store.label);
        }
        assert!(
            store
                .store
                .list_run_summaries(None)
                .await
                .unwrap()
                .is_empty(),
            "{}: a refused context must not create a run",
            store.label
        );
    }
}
