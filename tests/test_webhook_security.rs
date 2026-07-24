//! Webhook credential isolation, redaction, and fail-closed ingress tests.

use std::collections::HashMap;

use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use ironflow::engine::types::{Context, TaskState};
use ironflow::storage::StateStore as _;
use ironflow::storage::event_store::EventStore as _;

#[path = "support/webhook.rs"]
mod webhook_support;

use webhook_support::{
    PLATFORM_API_KEY, authenticated_json_request, authenticated_request, build_test_app, send_json,
    setup_flow_dir, webhook, write_flow,
};

#[tokio::test]
async fn webhook_denies_unconfigured_headers_by_default() {
    const SIGNATURE: &str = "unconfigured-signature-secret";

    let flows = tempfile::tempdir().unwrap();
    write_flow(
        flows.path(),
        "default_deny.lua",
        r#"
        local flow = Flow.new("default_deny")
        flow:step("inspect", function(ctx)
            return {
                signature_visible = ctx._headers["x-business-signature"] ~= nil,
                authorization_visible = ctx._headers.authorization ~= nil,
                cookie_visible = ctx._headers.cookie ~= nil
            }
        end)
        return flow
        "#,
    );
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("deny".to_string(), webhook("default_deny.lua", &[]))]),
    );
    let mut request = authenticated_json_request("/webhooks/deny", "{}");
    request.headers_mut().insert(
        HeaderName::from_static("x-business-signature"),
        HeaderValue::from_static(SIGNATURE),
    );
    request.headers_mut().insert(
        axum::http::header::COOKIE,
        HeaderValue::from_static("session=cookie-secret-12345"),
    );

    let (status, body) = send_json(&app.router, request).await;

    assert_eq!(status, StatusCode::OK);
    let run_id = body["run_id"].as_str().unwrap();
    let info = app.store.get_run_info(run_id).await.unwrap();
    assert_eq!(info.ctx["signature_visible"], false);
    assert_eq!(info.ctx["authorization_visible"], false);
    assert_eq!(info.ctx["cookie_visible"], false);
    assert!(!info.ctx.contains_key("_headers"));
    let raw = std::fs::read_to_string(app.store_dir.path().join(format!("{run_id}.json"))).unwrap();
    assert!(!raw.contains(SIGNATURE));
    assert!(!raw.contains("cookie-secret-12345"));
    assert!(!raw.contains(PLATFORM_API_KEY));
}

#[tokio::test]
async fn forwarded_header_is_usable_but_redacted_from_store_get_and_events() {
    const SIGNATURE: &str = "t=12345678,v1=business-signature-secret-12345";

    let flows = tempfile::tempdir().unwrap();
    write_flow(
        flows.path(),
        "signed.lua",
        r#"
        local flow = Flow.new("signed_webhook")
        flow:step("inspect_headers", function(ctx)
            local signature = ctx._headers["x-business-signature"]
            return {
                signature_valid = signature == "t=12345678,v1=business-signature-secret-12345",
                authorization_visible = ctx._headers.authorization ~= nil,
                api_key_visible = ctx._headers["x-api-key"] ~= nil,
                cookie_visible = ctx._headers.cookie ~= nil,
                unconfigured_visible = ctx._headers["x-unconfigured"] ~= nil,
                copied_signature = signature
            }
        end)
        flow:step("fail_safely", function(ctx)
            error("signature rejected: " .. ctx._headers["x-business-signature"])
        end):depends_on("inspect_headers")
        return flow
        "#,
    );
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([(
            "signed".to_string(),
            webhook("signed.lua", &["x-business-signature"]),
        )]),
    );
    let mut request = authenticated_json_request("/webhooks/signed", "{}");
    request.headers_mut().insert(
        HeaderName::from_static("x-api-key"),
        HeaderValue::from_static("secondary-platform-secret-12345"),
    );
    request.headers_mut().insert(
        HeaderName::from_static("x-business-signature"),
        HeaderValue::from_static(SIGNATURE),
    );
    request.headers_mut().insert(
        HeaderName::from_static("x-unconfigured"),
        HeaderValue::from_static("unconfigured-secret-12345"),
    );
    request.headers_mut().insert(
        axum::http::header::COOKIE,
        HeaderValue::from_static("session=cookie-secret-12345"),
    );

    let (status, body) = send_json(&app.router, request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "failed");
    let run_id = body["run_id"].as_str().unwrap();
    let info = app.store.get_run_info(run_id).await.unwrap();
    assert_eq!(info.ctx["signature_valid"], true);
    for key in [
        "authorization_visible",
        "api_key_visible",
        "cookie_visible",
        "unconfigured_visible",
    ] {
        assert_eq!(info.ctx[key], false);
    }
    assert_eq!(info.ctx["copied_signature"], "[REDACTED]");
    assert!(!info.ctx.contains_key("_headers"));
    assert!(
        info.tasks["fail_safely"]
            .error
            .as_deref()
            .unwrap()
            .contains("[REDACTED]")
    );

    let persisted =
        std::fs::read_to_string(app.store_dir.path().join(format!("{run_id}.json"))).unwrap();
    for secret in [
        SIGNATURE,
        "business-signature-secret-12345",
        PLATFORM_API_KEY,
        "secondary-platform-secret-12345",
        "cookie-secret-12345",
        "unconfigured-secret-12345",
    ] {
        assert!(!persisted.contains(secret), "state contained {secret}");
    }

    let events = app.events.list_since(run_id, None, 100).await.unwrap();
    let serialized_events = serde_json::to_string(&events).unwrap();
    assert!(serialized_events.contains("[REDACTED]"));
    assert!(!serialized_events.contains(SIGNATURE));

    let (get_status, public_run) = send_json(
        &app.router,
        authenticated_request(Method::GET, &format!("/runs/{run_id}"), Body::empty()),
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    let public_json = serde_json::to_string(&public_run).unwrap();
    assert!(!public_json.contains(SIGNATURE));
    assert!(public_json.contains("[REDACTED]"));
}

#[tokio::test]
async fn webhook_rejects_ambiguous_or_invalid_forwarded_headers_before_starting_run() {
    let flows = setup_flow_dir();
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([(
            "signed".to_string(),
            webhook("hello_world.lua", &["x-business-signature"]),
        )]),
    );

    let mut duplicate = authenticated_json_request("/webhooks/signed", "{}");
    for value in ["first-signature-value", "second-signature-value"] {
        duplicate.headers_mut().append(
            HeaderName::from_static("x-business-signature"),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    let (status, body) = send_json(&app.router, duplicate).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("must not be repeated")
    );

    let mut short = authenticated_json_request("/webhooks/signed", "{}");
    short.headers_mut().insert(
        HeaderName::from_static("x-business-signature"),
        HeaderValue::from_static("short"),
    );
    let (status, body) = send_json(&app.router, short).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("at least 8"));

    let mut non_text = authenticated_json_request("/webhooks/signed", "{}");
    non_text.headers_mut().insert(
        HeaderName::from_static("x-business-signature"),
        HeaderValue::from_bytes(&[0x80; 8]).unwrap(),
    );
    let (status, body) = send_json(&app.router, non_text).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("visible text"));

    assert!(app.store.list_runs(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn webhook_rejects_reserved_body_context_keys() {
    let flows = setup_flow_dir();
    let app = build_test_app(
        flows.path().to_path_buf(),
        HashMap::from([("hello".to_string(), webhook("hello_world.lua", &[]))]),
    );

    for key in ["_headers", "_webhook", "_flow_dir"] {
        let body = serde_json::json!({(key): "caller-controlled"}).to_string();
        let (status, response) = send_json(
            &app.router,
            authenticated_json_request("/webhooks/hello", &body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "reserved key {key}");
        assert!(response["error"].as_str().unwrap().contains(key));
    }

    assert!(app.store.list_runs(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn get_run_redacts_legacy_webhook_headers_and_copied_credentials() {
    const LEGACY_AUTH: &str = "Bearer old-platform-secret-token";
    const LEGACY_TOKEN: &str = "old-platform-secret-token";

    let flows = setup_flow_dir();
    let app = build_test_app(flows.path().to_path_buf(), HashMap::new());
    let legacy_ctx = Context::from([
        (
            "_headers".to_string(),
            serde_json::json!({
                "authorization": LEGACY_AUTH,
                "cookie": "session=old-cookie-secret",
                "content-length": "2"
            }),
        ),
        ("auth_token".to_string(), serde_json::json!(LEGACY_TOKEN)),
    ]);
    app.store
        .init_run("legacy-webhook-run", "legacy", &legacy_ctx)
        .await
        .unwrap();
    let mut task = TaskState::new("check", "code");
    task.error = Some(format!("request rejected with {LEGACY_TOKEN}"));
    task.output = Some(serde_json::json!({"copied": LEGACY_AUTH}));
    app.store
        .upsert_task("legacy-webhook-run", &task)
        .await
        .unwrap();

    let (status, body) = send_json(
        &app.router,
        authenticated_request(Method::GET, "/runs/legacy-webhook-run", Body::empty()),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let serialized = serde_json::to_string(&body).unwrap();
    for secret in ["_headers", LEGACY_AUTH, LEGACY_TOKEN, "old-cookie-secret"] {
        assert!(!serialized.contains(secret));
    }
    assert_eq!(body["id"], "legacy-webhook-run");
    assert!(serialized.contains("[REDACTED]"));
}
