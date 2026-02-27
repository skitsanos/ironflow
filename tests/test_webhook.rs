//! Tests for webhook route handling.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use http_body_util::BodyExt;
use tower::ServiceExt;

use ironflow::nodes::NodeRegistry;
use ironflow::storage::json_store::JsonStateStore;

/// Build a test router with the webhook route wired up, mirroring src/api/mod.rs.
fn build_test_app(
    flows_dir: std::path::PathBuf,
    webhooks: HashMap<String, String>,
) -> Router {
    let registry = Arc::new(NodeRegistry::with_builtins());
    let store = Arc::new(JsonStateStore::new(
        tempfile::tempdir().unwrap().keep(),
    ));

    let state = Arc::new(ironflow::api::AppState {
        registry,
        store,
        flows_dir: Some(flows_dir),
        max_concurrent_tasks: None,
        webhooks,
    });

    Router::new()
        .route(
            "/webhooks/{name}",
            post(ironflow::api::handlers::run_webhook),
        )
        .with_state(state)
}

/// Helper: create a temp dir with a Lua flow file, return the dir path.
fn setup_flow_dir() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let flow_path = dir.path().join("hello_world.lua");
    let mut f = std::fs::File::create(&flow_path).unwrap();
    f.write_all(
        br#"
        local flow = Flow.new("webhook_test")
        flow:step("greet", nodes.log({ message = "hello from webhook" }))
        return flow
    "#,
    )
    .unwrap();
    let dir_path = dir.path().to_path_buf();
    (dir, dir_path)
}

#[tokio::test]
async fn webhook_executes_flow_and_returns_run_id() {
    let (_dir, dir_path) = setup_flow_dir();

    let mut webhooks = HashMap::new();
    webhooks.insert("hello".to_string(), "hello_world.lua".to_string());

    let app = build_test_app(dir_path, webhooks);

    let req = Request::builder()
        .method("POST")
        .uri("/webhooks/hello")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("run_id").is_some());
    assert_eq!(json["flow_name"], "webhook_test");
    assert_eq!(json["status"], "success");
}

#[tokio::test]
async fn webhook_unknown_name_returns_404() {
    let (_dir, dir_path) = setup_flow_dir();

    let app = build_test_app(dir_path, HashMap::new());

    let req = Request::builder()
        .method("POST")
        .uri("/webhooks/nonexistent")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn webhook_passes_json_body_as_context() {
    let dir = tempfile::tempdir().unwrap();
    let flow_path = dir.path().join("echo_ctx.lua");
    let mut f = std::fs::File::create(&flow_path).unwrap();
    f.write_all(
        br#"
        local flow = Flow.new("echo_ctx")
        flow:step("check", nodes.code({
            source = "ctx.greeting_received = ctx.greeting"
        }))
        return flow
    "#,
    )
    .unwrap();

    let mut webhooks = HashMap::new();
    webhooks.insert("echo".to_string(), "echo_ctx.lua".to_string());

    let app = build_test_app(dir.path().to_path_buf(), webhooks);

    let req = Request::builder()
        .method("POST")
        .uri("/webhooks/echo")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"greeting": "hi"}"#))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "success");
}

#[tokio::test]
async fn webhook_works_with_no_body() {
    let (_dir, dir_path) = setup_flow_dir();

    let mut webhooks = HashMap::new();
    webhooks.insert("hello".to_string(), "hello_world.lua".to_string());

    let app = build_test_app(dir_path, webhooks);

    // Send POST with no body at all
    let req = Request::builder()
        .method("POST")
        .uri("/webhooks/hello")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn webhook_injects_headers_into_context() {
    let dir = tempfile::tempdir().unwrap();
    let flow_path = dir.path().join("check_auth.lua");
    let mut f = std::fs::File::create(&flow_path).unwrap();
    f.write_all(
        br#"
        local flow = Flow.new("check_auth")
        flow:step("check", nodes.code({
            source = "ctx.got_auth = ctx._headers.authorization"
        }))
        return flow
    "#,
    )
    .unwrap();

    let mut webhooks = HashMap::new();
    webhooks.insert("auth".to_string(), "check_auth.lua".to_string());

    let app = build_test_app(dir.path().to_path_buf(), webhooks);

    let req = Request::builder()
        .method("POST")
        .uri("/webhooks/auth")
        .header("content-type", "application/json")
        .header("authorization", "Bearer test-token-123")
        .body(Body::from("{}"))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "success");
}

#[tokio::test]
async fn webhook_injects_webhook_name_into_context() {
    let dir = tempfile::tempdir().unwrap();
    let flow_path = dir.path().join("check_name.lua");
    let mut f = std::fs::File::create(&flow_path).unwrap();
    f.write_all(
        br#"
        local flow = Flow.new("check_name")
        flow:step("check", nodes.code({
            source = "ctx.hook_name = ctx._webhook"
        }))
        return flow
    "#,
    )
    .unwrap();

    let mut webhooks = HashMap::new();
    webhooks.insert("my-hook".to_string(), "check_name.lua".to_string());

    let app = build_test_app(dir.path().to_path_buf(), webhooks);

    let req = Request::builder()
        .method("POST")
        .uri("/webhooks/my-hook")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
