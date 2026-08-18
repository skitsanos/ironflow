// IF-045: a file-mode flow load that fails to parse must not echo the file's
// contents in the public error response (mlua's syntax error includes a
// `near '<token>'` snippet of the file).

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use http_body_util::BodyExt;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use tower::ServiceExt;

fn app(store: Arc<dyn StateStore>) -> Router {
    let state = Arc::new(ironflow::api::AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store,
        event_store: Arc::new(MemoryEventStore::new()),
        flows_dir: None,
        max_concurrent_tasks: None,
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: std::collections::HashMap::new(),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
        metrics: None,
    });
    Router::new()
        .route("/flows/run", post(ironflow::api::handlers::run_flow))
        .with_state(state)
}

#[tokio::test]
async fn file_mode_load_error_does_not_leak_file_contents() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(dir.path()));

    // A non-Lua file whose contents surface in the Lua lexer error. A token
    // that starts like a number is reported verbatim: "malformed number near
    // '<token>'", so the secret leaks into the parse error if it is echoed.
    let secret = "1LEAKED_secret_abc123xyz";
    let file = dir.path().join("secret.txt");
    std::fs::write(&file, secret).unwrap();

    let body = serde_json::json!({ "file": file.to_str().unwrap() }).to_string();
    let response = app(store)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/flows/run")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    // The public response must not echo the Lua lexer's file-derived detail.
    assert!(
        !text.contains("malformed number") && !text.contains("near '"),
        "response leaked the parse-error detail: {text}"
    );
    assert!(
        !text.contains("LEAKED"),
        "response leaked file contents: {text}"
    );
}

// IF-038: a submitted flow whose parse loops must be bounded (parsed off the
// async runtime under execution limits) and return an error, not hang.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flow_parse_with_runaway_loop_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(dir.path()));

    let body = serde_json::json!({ "source": "while true do end" }).to_string();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        app(store).oneshot(
            Request::builder()
                .method("POST")
                .uri("/flows/run")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        ),
    )
    .await
    .expect("parse must be bounded, not hang")
    .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
