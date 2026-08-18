//! Integration coverage for execution-only webhook headers in child workflows.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use http_body_util::BodyExt as _;
use ironflow::api::{AppState, WebhookConfig};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore as _;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use tower::ServiceExt as _;

const SIGNATURE: &str = "if011-composition-secret-sentinel-4f08c7";

#[tokio::test]
async fn forwarded_signature_stays_execution_only_across_subworkflow_recovery() {
    let flow_dir = tempfile::tempdir().unwrap();
    let run_dir = tempfile::tempdir().unwrap();

    std::fs::write(
        flow_dir.path().join("parent.lua"),
        r#"
        local flow = Flow.new("webhook_composition_parent")
        flow:step("child", nodes.subworkflow({
            flow = "child.lua",
            output_key = "child_out"
        }))
        return flow
        "#,
    )
    .unwrap();
    std::fs::write(
        flow_dir.path().join("child.lua"),
        r#"
        local flow = Flow.new("webhook_composition_child")
        flow:step("fail_with_signature", function(ctx)
            error("child received signature: " .. ctx._headers["stripe-signature"])
        end):on_error("recover")
        flow:step("recover", function(ctx)
            return {
                signature_seen = ctx._headers["stripe-signature"] ~= nil,
                copied_signature = ctx._headers["stripe-signature"],
                recovered_error = ctx._error_message
            }
        end)
        return flow
        "#,
    )
    .unwrap();

    let store = Arc::new(JsonStateStore::new(run_dir.path()));
    let event_store = Arc::new(MemoryEventStore::new());
    let webhook = WebhookConfig::new("parent.lua", ["stripe-signature".to_string()]).unwrap();
    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store: store.clone(),
        event_store,
        flows_dir: Some(flow_dir.path().to_path_buf()),
        max_concurrent_tasks: None,
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: HashMap::from([("signed".to_string(), webhook)]),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
        metrics: None,
    });
    let app = Router::new()
        .route(
            "/webhooks/{name}",
            post(ironflow::api::handlers::run_webhook),
        )
        .with_state(state);

    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/signed")
        .header("content-type", "application/json")
        .header("stripe-signature", SIGNATURE)
        .body(Body::from("{}"))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let response_body = response.into_body().collect().await.unwrap().to_bytes();
    let response_json: serde_json::Value = serde_json::from_slice(&response_body).unwrap();
    let response_text = String::from_utf8(response_body.to_vec()).unwrap();
    assert!(!response_text.contains(SIGNATURE));
    assert_eq!(response_json["status"], "success");

    let run_id = response_json["run_id"].as_str().unwrap();
    let run_info = store.get_run_info(run_id).await.unwrap();
    let stored_json = serde_json::to_string(&run_info).unwrap();
    assert!(!stored_json.contains(SIGNATURE));
    assert!(!stored_json.contains("\"_headers\""));
    assert_eq!(run_info.ctx["child_out"]["signature_seen"], true);
    assert_eq!(run_info.ctx["child_out"]["copied_signature"], "[REDACTED]");
    assert!(
        run_info.ctx["child_out"]["recovered_error"]
            .as_str()
            .unwrap()
            .contains("[REDACTED]")
    );

    let raw_run = tokio::fs::read_to_string(run_dir.path().join(format!("{run_id}.json")))
        .await
        .unwrap();
    assert!(!raw_run.contains(SIGNATURE));
    assert!(!raw_run.contains("\"_headers\""));
}
