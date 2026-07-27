//! `allow_adhoc_flows` (IF-054).
//!
//! `POST /flows/run` with inline `source` is arbitrary workflow execution: the
//! caller picks the nodes, so an API key that reaches it can read or write any
//! path the process can. Deployments that expose a fixed set of flows need to be
//! able to turn that off without losing `file`-based execution, which is already
//! confined to `flows_dir`.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use tower::ServiceExt;

/// Returns the router together with the store's TempDir — hold the guard for the
/// duration of the test so the run store is not removed while the server uses it.
fn app(
    allow_adhoc_flows: bool,
    flows_dir: Option<std::path::PathBuf>,
) -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(dir.path()));
    let state = Arc::new(ironflow::api::AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store,
        event_store: Arc::new(MemoryEventStore::new()),
        flows_dir,
        max_concurrent_tasks: None,
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: std::collections::HashMap::new(),
        allow_adhoc_flows,
    });
    let router = Router::new()
        .route("/flows/run", post(ironflow::api::handlers::run_flow))
        .with_state(state);
    (router, dir)
}

async fn post_run(app: Router, body: serde_json::Value) -> (StatusCode, String) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/flows/run")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

const INLINE_FLOW: &str = r#"
local flow = Flow.new("adhoc")
flow:step("s", function(ctx) return { ok = true } end)
return flow
"#;

#[tokio::test]
async fn inline_source_is_rejected_when_adhoc_flows_are_disabled() {
    let (router, _store) = app(false, None);
    let (status, body) = post_run(router, serde_json::json!({ "source": INLINE_FLOW })).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
    assert!(
        body.contains("Inline flow source is disabled"),
        "body: {body}"
    );
}

#[tokio::test]
async fn inline_source_base64_is_rejected_when_adhoc_flows_are_disabled() {
    let (router, _store) = app(false, None);
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(INLINE_FLOW);
    let (status, body) = post_run(router, serde_json::json!({ "source_base64": b64 })).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
}

#[tokio::test]
async fn inline_source_is_allowed_by_default() {
    let (router, _store) = app(true, None);
    let (status, body) = post_run(router, serde_json::json!({ "source": INLINE_FLOW })).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
}

#[tokio::test]
async fn file_based_flows_still_run_when_adhoc_flows_are_disabled() {
    let flows = tempfile::tempdir().unwrap();
    std::fs::write(flows.path().join("ok.lua"), INLINE_FLOW).unwrap();
    let (router, _store) = app(false, Some(flows.path().to_path_buf()));

    let (status, body) = post_run(router, serde_json::json!({ "file": "ok.lua" })).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
}

#[tokio::test]
async fn disabling_adhoc_flows_does_not_weaken_the_flows_dir_boundary() {
    let flows = tempfile::tempdir().unwrap();
    std::fs::write(flows.path().join("ok.lua"), INLINE_FLOW).unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("evil.lua"), INLINE_FLOW).unwrap();
    let (router, _store) = app(false, Some(flows.path().to_path_buf()));

    let (status, _) = post_run(
        router,
        serde_json::json!({ "file": outside.path().join("evil.lua").to_string_lossy() }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
