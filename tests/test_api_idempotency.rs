use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use http_body_util::BodyExt as _;
use ironflow::api::{AppState, ServiceLifecycle};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use tower::ServiceExt as _;

fn app(store: Arc<JsonStateStore>) -> Router {
    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store,
        event_store: Arc::new(MemoryEventStore::new()),
        flows_dir: None,
        max_concurrent_tasks: Some(1),
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: std::collections::HashMap::new(),
        allow_adhoc_flows: true,
        lifecycle: ServiceLifecycle::default(),
        metrics: None,
    });
    Router::new()
        .route("/flows/run", post(ironflow::api::handlers::run_flow))
        .with_state(state)
}

fn request(key: &str, message: &str) -> Request<Body> {
    let source = r#"
        local flow = Flow.new("idempotent")
        flow:step("log", nodes.log({ message = "${ctx.message}" }))
        return flow
    "#;
    Request::builder()
        .method("POST")
        .uri("/flows/run")
        .header("content-type", "application/json")
        .header("idempotency-key", key)
        .body(Body::from(
            serde_json::json!({
                "source": source,
                "context": {"message": message}
            })
            .to_string(),
        ))
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn same_key_converges_and_different_payload_conflicts() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(directory.path()));
    let app = app(store.clone());

    let (left, right) = tokio::join!(
        app.clone().oneshot(request("job:customer-42", "hello")),
        app.clone().oneshot(request("job:customer-42", "hello")),
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.status(), StatusCode::OK);
    assert_eq!(right.status(), StatusCode::OK);
    let left = response_json(left).await;
    let right = response_json(right).await;
    assert_eq!(left["run_id"], right["run_id"]);
    assert_eq!(store.list_runs(None).await.unwrap().len(), 1);

    let conflict = app
        .clone()
        .oneshot(request("job:customer-42", "different"))
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);

    let invalid = app
        .oneshot(request("contains spaces", "hello"))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
}
