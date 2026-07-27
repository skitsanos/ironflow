use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::null_store::NullStateStore;
use tower::ServiceExt;

fn app() -> Router {
    let state = Arc::new(ironflow::api::AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store: Arc::new(NullStateStore::new()),
        event_store: Arc::new(MemoryEventStore::new()),
        flows_dir: None,
        max_concurrent_tasks: None,
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: std::collections::HashMap::new(),
        allow_adhoc_flows: true,
    });

    Router::new()
        .route(
            "/runs/{id}",
            get(ironflow::api::handlers::get_run).delete(ironflow::api::handlers::delete_run),
        )
        .route(
            "/runs/{id}/events",
            get(ironflow::api::handlers::run_events),
        )
        .with_state(state)
}

async fn request(method: Method, uri: &str) -> axum::response::Response {
    app()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

async fn assert_error(method: Method, uri: &str, status: StatusCode, code: &str) {
    let response = request(method, uri).await;
    assert_eq!(response.status(), status, "unexpected status for {uri}");
    let body = response_json(response).await;
    assert_eq!(body["code"], code, "unexpected response for {uri}");
}

#[tokio::test]
async fn every_public_run_resource_rejects_invalid_ids_before_storage() {
    for (method, uri) in [
        (Method::GET, "/runs/%2E%2E"),
        (Method::DELETE, "/runs/%2E%2E"),
        (Method::GET, "/runs/%2E%2E/events"),
    ] {
        assert_error(method, uri, StatusCode::BAD_REQUEST, "bad_request").await;
    }
}

#[tokio::test]
async fn traversal_and_percent_decoded_ids_are_bad_requests() {
    let oversized = format!("/runs/{}", "a".repeat(129));
    let invalid = [
        "/runs/-leading",
        "/runs/trailing_",
        "/runs/run%2Eid",
        "/runs/run%2Foutside",
        "/runs/%2E%2E%2Foutside",
        "/runs/run%5Coutside",
        "/runs/run%20id",
        "/runs/%C3%A9",
        "/runs/run%00id",
        oversized.as_str(),
    ];

    for uri in invalid {
        assert_error(Method::GET, uri, StatusCode::BAD_REQUEST, "bad_request").await;
    }
}

#[tokio::test]
async fn canonical_but_absent_ids_remain_not_found() {
    let max_length = format!("/runs/{}", "a".repeat(128));
    for uri in ["/runs/valid-Run_123", max_length.as_str()] {
        assert_error(Method::GET, uri, StatusCode::NOT_FOUND, "not_found").await;
    }

    for (method, uri) in [
        (Method::GET, "/runs/absent-run"),
        (Method::DELETE, "/runs/absent-run"),
        (Method::GET, "/runs/absent-run/events"),
    ] {
        assert_error(method, uri, StatusCode::NOT_FOUND, "not_found").await;
    }
}
