//! IF-181: a webhook or `/flows/run` body nested deeper than the state stores
//! can read back is a 400, not a run record that nothing can read.

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

#[path = "support/json_depth.rs"]
mod depth_support;

use depth_support::{ADMITTED_DEPTH, REPORTED_DEPTH, context_text, nested_value};

const LIMIT_MESSAGE: &str = "nested deeper than 123 levels";

#[tokio::test]
async fn webhook_refuses_a_body_the_store_could_not_read_back() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = setup_flow_dir();
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("hello".to_string(), webhook("hello_world.lua", &[]))]),
    );

    for depth in [REPORTED_DEPTH, ADMITTED_DEPTH + 1] {
        let (status, body) = send_json(
            &app.router,
            authenticated_json_request("/webhooks/hello", &context_text(depth)),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "depth {depth}: {body}");
        assert_eq!(body["code"], "bad_request");
        let error = body["error"].as_str().unwrap_or_default();
        assert!(error.contains(LIMIT_MESSAGE), "depth {depth}: {error}");
    }
    assert!(
        app.store.list_run_summaries(None).await.unwrap().is_empty(),
        "a refused body must not create a run"
    );
}

#[tokio::test]
async fn webhook_stores_and_reads_back_a_body_at_the_limit() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = setup_flow_dir();
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("hello".to_string(), webhook("hello_world.lua", &[]))]),
    );

    let (status, body) = send_json(
        &app.router,
        authenticated_json_request("/webhooks/hello", &context_text(ADMITTED_DEPTH)),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "success");
    let info = app
        .store
        .get_run_info(body["run_id"].as_str().unwrap())
        .await
        .unwrap();
    assert!(info.ctx["a"] == nested_value(ADMITTED_DEPTH));
}

#[tokio::test]
async fn run_flow_refuses_a_context_the_store_could_not_read_back() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(store_dir.path()));
    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store: store.clone(),
        event_store: Arc::new(MemoryEventStore::new()),
        flows_dir: None,
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
    let source = r#"local flow = Flow.new("adhoc") flow:step("greet", nodes.log({ message = "hi" })) return flow"#;

    // The request object wraps the context once more, so the parser already
    // refuses the depth reported in IF-181 here; one level past the shared
    // limit is the deepest case this route can still reach.
    let request = format!(
        "{{\"source\":{},\"context\":{}}}",
        serde_json::to_string(source).unwrap(),
        context_text(ADMITTED_DEPTH + 1)
    );
    let (status, body) =
        send_json(&router, authenticated_json_request("/flows/run", &request)).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let error = body["error"].as_str().unwrap_or_default();
    assert!(error.contains(LIMIT_MESSAGE), "{error}");
    assert!(
        store.list_run_summaries(None).await.unwrap().is_empty(),
        "a refused context must not create a run"
    );
}
