//! IF-203 (b): `POST /flows/run` answers an invalid flow with a 400 that names
//! the problem, as `POST /flows/validate` does.

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
    PROCESS_ADMISSION_LOCK, authenticated_json_request, build_test_app, send_json, webhook,
    write_flow,
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
        .route(
            "/flows/validate",
            post(ironflow::api::handlers::validate_flow),
        )
        .with_state(state);
    App {
        router,
        store,
        _store_dir: store_dir,
    }
}

const UNDEFINED_DEPENDENCY: &str = r#"
local flow = Flow.new("broken")
flow:step("report", nodes.log({ message = "hi" })):depends_on("missing")
return flow
"#;
const DEPENDENCY_ERROR: &str = "Step 'report' depends on 'missing', which does not exist";

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
async fn an_inline_flow_with_an_undefined_dependency_is_a_bad_request() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let app = app(None);

    let (status, validation) = post_json(
        &app,
        "/flows/validate",
        serde_json::json!({ "source": UNDEFINED_DEPENDENCY }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(validation["valid"], false);
    assert_eq!(validation["errors"], serde_json::json!([DEPENDENCY_ERROR]));

    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        UNDEFINED_DEPENDENCY,
    );
    for request in [
        serde_json::json!({ "source": UNDEFINED_DEPENDENCY }),
        serde_json::json!({ "source_base64": encoded }),
    ] {
        let (status, body) = post_json(&app, "/flows/run", request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["code"], "bad_request");
        let error = body["error"].as_str().unwrap_or_default();
        assert!(error.contains(DEPENDENCY_ERROR), "{error}");
    }
    assert!(
        app.store.list_run_summaries(None).await.unwrap().is_empty(),
        "an invalid flow must not create a run"
    );
}

#[tokio::test]
async fn every_validation_error_is_reported_and_unknown_node_types_are_refused() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let app = app(None);
    let source = r#"
        local flow = Flow.new("broken")
        flow:step("a", { _node_type = "no_such_node" }):depends_on("missing")
        flow:step("b", nodes.log({ message = "hi" })):retries(101, 0)
        return flow
    "#;

    let (status, body) =
        post_json(&app, "/flows/run", serde_json::json!({ "source": source })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let error = body["error"].as_str().unwrap_or_default();
    for expected in [
        "Step 'a' uses unknown node type 'no_such_node'",
        "Step 'a' depends on 'missing', which does not exist",
        "Step 'b' retry count is too large (max 100)",
    ] {
        assert!(error.contains(expected), "{error}");
    }
    assert!(app.store.list_run_summaries(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_file_flow_with_an_undefined_dependency_is_a_bad_request() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = tempfile::tempdir().unwrap();
    write_flow(flows.path(), "broken.lua", UNDEFINED_DEPENDENCY);
    let app = app(Some(flows.path().to_path_buf()));

    let (status, body) = post_json(
        &app,
        "/flows/run",
        serde_json::json!({ "file": "broken.lua" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let error = body["error"].as_str().unwrap_or_default();
    assert!(error.contains(DEPENDENCY_ERROR), "{error}");
    assert!(app.store.list_run_summaries(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_webhook_whose_flow_fails_validation_is_a_bad_request_without_detail() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = tempfile::tempdir().unwrap();
    write_flow(flows.path(), "broken.lua", UNDEFINED_DEPENDENCY);
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("broken".to_string(), webhook("broken.lua", &[]))]),
    );

    let (status, body) = send_json(
        &app.router,
        authenticated_json_request("/webhooks/broken", "{}"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // The flow belongs to the server's configuration, not to the caller.
    assert_eq!(body["error"], "Webhook flow failed validation");
    assert!(app.store.list_run_summaries(None).await.unwrap().is_empty());
}
