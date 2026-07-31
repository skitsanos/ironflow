//! `allow_adhoc_flows` (IF-054).
//!
//! `POST /flows/run` and `POST /flows/validate` both evaluate the supplied Lua
//! chunk. Deployments that expose a fixed set of flows need to turn inline
//! evaluation off without losing `file` mode, which is already confined to
//! `flows_dir`.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use tower::ServiceExt;

// API flow-loading admission is process-global. Serialize only the tests in
// this binary that actually enter the parser so they do not consume the two
// default permits concurrently and turn an expected success into a 503.
static FLOW_EVALUATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        .route(
            "/flows/validate",
            post(ironflow::api::handlers::validate_flow),
        )
        .with_state(state);
    (router, dir)
}

async fn post_flow(app: Router, path: &str, body: serde_json::Value) -> (StatusCode, String) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
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

async fn post_run(app: Router, body: serde_json::Value) -> (StatusCode, String) {
    post_flow(app, "/flows/run", body).await
}

async fn post_validate(app: Router, body: serde_json::Value) -> (StatusCode, String) {
    post_flow(app, "/flows/validate", body).await
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
async fn validation_does_not_evaluate_inline_source_when_adhoc_flows_are_disabled() {
    let (router, _store) = app(false, None);
    // Before IF-061, validation evaluated this top-level chunk and returned the
    // process environment value in its public Lua error.
    let source = r#"error("IF061_ENV_EXFIL:" .. (env("PATH") or "unavailable"))"#;
    let (status, body) = post_validate(router, serde_json::json!({ "source": source })).await;

    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
    assert!(
        body.contains("Inline flow source is disabled"),
        "body: {body}"
    );
    assert!(!body.contains("IF061_ENV_EXFIL"), "body: {body}");
}

#[tokio::test]
async fn validation_rejects_inline_base64_when_adhoc_flows_are_disabled() {
    let (router, _store) = app(false, None);
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(INLINE_FLOW);
    let (status, body) = post_validate(router, serde_json::json!({ "source_base64": b64 })).await;

    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
}

#[tokio::test]
async fn oversized_base64_source_is_rejected_by_decoded_size() {
    use base64::Engine as _;

    let (router, _store) = app(true, None);
    let oversized = vec![b'x'; 1024 * 1024 + 1];
    let encoded = base64::engine::general_purpose::STANDARD.encode(oversized);
    let (status, body) =
        post_validate(router, serde_json::json!({ "source_base64": encoded })).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
    assert!(body.contains("IRONFLOW_MAX_FLOW_SOURCE_BYTES"), "{body}");
}

#[tokio::test]
async fn file_based_flows_still_validate_when_adhoc_flows_are_disabled() {
    let _flow_evaluation = FLOW_EVALUATION_LOCK.lock().await;
    let flows = tempfile::tempdir().unwrap();
    std::fs::write(flows.path().join("ok.lua"), INLINE_FLOW).unwrap();
    let (router, _store) = app(false, Some(flows.path().to_path_buf()));

    let (status, body) = post_validate(router, serde_json::json!({ "file": "ok.lua" })).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let response: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["valid"], true);
    assert_eq!(response["flow_name"], "adhoc");
}

#[tokio::test]
async fn disabled_adhoc_policy_without_flows_dir_cannot_select_arbitrary_files() {
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), INLINE_FLOW).unwrap();
    let (router, _store) = app(false, None);

    let (status, body) = post_validate(
        router,
        serde_json::json!({ "file": outside.path().display().to_string() }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
    assert!(body.contains("requires a configured flows_dir"), "{body}");
}

#[tokio::test]
async fn inline_source_is_allowed_by_default() {
    let _flow_evaluation = FLOW_EVALUATION_LOCK.lock().await;
    let (router, _store) = app(true, None);
    let (status, body) = post_run(router, serde_json::json!({ "source": INLINE_FLOW })).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
}

#[tokio::test]
async fn file_based_flows_still_run_when_adhoc_flows_are_disabled() {
    let _flow_evaluation = FLOW_EVALUATION_LOCK.lock().await;
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
    assert_eq!(status, StatusCode::NOT_FOUND);
}
