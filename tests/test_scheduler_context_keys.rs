//! IF-203 (c): API requests and webhook bodies cannot define the context keys
//! that record which schedule occurrence started a run.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::post;
use ironflow::api::AppState;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore as _;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;

use crate::webhook_support::{
    PROCESS_ADMISSION_LOCK, authenticated_json_request, build_test_app, send_json, setup_flow_dir,
    webhook,
};

struct App {
    router: Router,
    store: Arc<JsonStateStore>,
    _store_dir: tempfile::TempDir,
}

fn app(flows_dir: Option<std::path::PathBuf>) -> App {
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(store_dir.path()));
    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store: store.clone(),
        event_store: Arc::new(MemoryEventStore::new()),
        flows_dir,
        max_concurrent_tasks: None,
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: HashMap::new(),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
        metrics: None,
    });
    let router = Router::new()
        .route("/flows/run", post(ironflow::api::handlers::run_flow))
        .with_state(state);
    App {
        router,
        store,
        _store_dir: store_dir,
    }
}

async fn post_json(
    app: &App,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    send_json(
        &app.router,
        authenticated_json_request(uri, &body.to_string()),
    )
    .await
}

#[tokio::test]
async fn run_flow_refuses_the_context_keys_the_scheduler_owns() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let app = app(None);
    let source = r#"local flow = Flow.new("adhoc") flow:step("greet", nodes.log({ message = "hi" })) return flow"#;

    for key in ["_schedule", "_schedule_instant"] {
        let (status, body) = post_json(
            &app,
            "/flows/run",
            serde_json::json!({ "source": source, "context": { (key): "nightly_report" } }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "reserved key {key}: {body}"
        );
        let error = body["error"].as_str().unwrap_or_default();
        assert!(error.contains(&format!("reserved key '{key}'")), "{error}");
    }
    assert!(
        app.store.list_run_summaries(None).await.unwrap().is_empty(),
        "a refused context must not create a run"
    );

    // Other underscore-prefixed keys stay available to callers.
    let (status, body) = post_json(
        &app,
        "/flows/run",
        serde_json::json!({ "source": source, "context": { "_trace": "abc" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn a_webhook_refuses_the_context_keys_the_scheduler_owns() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = setup_flow_dir();
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("hello".to_string(), webhook("hello_world.lua", &[]))]),
    );

    for key in ["_schedule", "_schedule_instant"] {
        let body = serde_json::json!({ (key): "nightly_report" }).to_string();
        let (status, response) = send_json(
            &app.router,
            authenticated_json_request("/webhooks/hello", &body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "reserved key {key}");
        assert!(response["error"].as_str().unwrap().contains(key));
    }
    assert!(app.store.list_run_summaries(None).await.unwrap().is_empty());
}
