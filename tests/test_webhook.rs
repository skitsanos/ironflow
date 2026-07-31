//! Baseline end-to-end tests for webhook routing and durable business context.

use std::collections::HashMap;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use ironflow::storage::StateStore as _;

#[path = "support/webhook.rs"]
mod webhook_support;

use webhook_support::{
    PLATFORM_API_KEY, PROCESS_ADMISSION_LOCK, authenticated_json_request, authenticated_request,
    build_test_app, send_json, setup_flow_dir, webhook, write_flow,
};

#[tokio::test]
async fn webhook_executes_flow_and_returns_persisted_run() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = setup_flow_dir();
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("hello".to_string(), webhook("hello_world.lua", &[]))]),
    );

    let (status, body) = send_json(
        &app.router,
        authenticated_json_request("/webhooks/hello", "{}"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["flow_name"], "webhook_test");
    assert_eq!(body["status"], "success");
    let run_id = body["run_id"].as_str().unwrap();
    let info = app.store.get_run_info(run_id).await.unwrap();
    assert_eq!(info.flow_name, "webhook_test");
    assert!(info.status.is_terminal());
}

#[tokio::test]
async fn webhook_routes_are_protected_by_platform_authentication() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = setup_flow_dir();
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("hello".to_string(), webhook("hello_world.lua", &[]))]),
    );

    let (missing_status, _) = send_json(
        &app.router,
        Request::builder()
            .method(Method::POST)
            .uri("/webhooks/hello")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(missing_status, StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .method(Method::POST)
        .uri("/webhooks/hello")
        .header("x-api-key", PLATFORM_API_KEY)
        .body(Body::empty())
        .unwrap();
    let (valid_status, _) = send_json(&app.router, request).await;
    assert_eq!(valid_status, StatusCode::OK);
}

#[tokio::test]
async fn webhook_unknown_name_returns_404() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = setup_flow_dir();
    let app = build_test_app(flows.path().to_path_buf(), HashMap::new());

    let (status, _) = send_json(
        &app.router,
        authenticated_json_request("/webhooks/nonexistent", "{}"),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn webhook_passes_json_body_and_webhook_name_as_durable_context() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = tempfile::tempdir().unwrap();
    write_flow(
        flows.path(),
        "echo_ctx.lua",
        r#"
        local flow = Flow.new("echo_ctx")
        flow:step("check", function(ctx)
            return {
                greeting_received = ctx.greeting,
                hook_name = ctx._webhook
            }
        end)
        return flow
        "#,
    );
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("echo".to_string(), webhook("echo_ctx.lua", &[]))]),
    );

    let (status, body) = send_json(
        &app.router,
        authenticated_json_request("/webhooks/echo", r#"{"greeting":"hi"}"#),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let info = app
        .store
        .get_run_info(body["run_id"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(info.ctx["greeting_received"], "hi");
    assert_eq!(info.ctx["hook_name"], "echo");
    assert_eq!(info.ctx["_webhook"], "echo");
}

#[tokio::test]
async fn webhook_works_with_no_body() {
    let _admission = PROCESS_ADMISSION_LOCK.lock().await;
    let flows = setup_flow_dir();
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("hello".to_string(), webhook("hello_world.lua", &[]))]),
    );

    let (status, _) = send_json(
        &app.router,
        authenticated_request(Method::POST, "/webhooks/hello", Body::empty()),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}
