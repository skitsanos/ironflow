use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Method, Request, StatusCode};
use axum::middleware;
use axum::routing::{get, post};
use http_body_util::BodyExt as _;
use ironflow::api::{ApiAuth, AppState, WebhookConfig};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use tower::ServiceExt as _;

pub const PLATFORM_API_KEY: &str = "platform-api-secret-12345";

// Webhook integration cases create independent routers, while flow parsing is
// intentionally admitted by one process-wide semaphore. Tests sharing this
// support module serialize those synthetic server lifetimes.
pub static PROCESS_ADMISSION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[allow(dead_code)] // Each integration-test crate uses a different subset.
pub struct TestApp {
    pub router: Router,
    pub store: Arc<JsonStateStore>,
    pub events: Arc<MemoryEventStore>,
    pub store_dir: tempfile::TempDir,
}

pub fn webhook(flow: &str, forward_headers: &[&str]) -> WebhookConfig {
    WebhookConfig::new(
        flow,
        forward_headers.iter().map(|header| (*header).to_string()),
    )
    .unwrap()
}

/// Build protected webhook/run-detail routes with the production API-key
/// middleware ordering.
pub fn build_test_app(flows_dir: PathBuf, webhooks: HashMap<String, WebhookConfig>) -> TestApp {
    let registry = Arc::new(NodeRegistry::with_builtins());
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(store_dir.path()));
    let events = Arc::new(MemoryEventStore::new());

    let state = Arc::new(AppState {
        registry,
        store: store.clone(),
        event_store: events.clone(),
        flows_dir: Some(flows_dir),
        max_concurrent_tasks: None,
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks,
        allow_adhoc_flows: true,
    });

    let protected = Router::new()
        .route(
            "/webhooks/{name}",
            post(ironflow::api::handlers::run_webhook),
        )
        .route("/runs/{id}", get(ironflow::api::handlers::get_run))
        .layer(middleware::from_fn_with_state(
            ApiAuth::new(PLATFORM_API_KEY),
            ironflow::api::require_api_key,
        ));

    TestApp {
        router: protected.with_state(state),
        store,
        events,
        store_dir,
    }
}

pub fn write_flow(dir: &Path, name: &str, source: &str) {
    std::fs::write(dir.join(name), source).unwrap();
}

pub fn authenticated_request(method: Method, uri: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {PLATFORM_API_KEY}"))
        .body(body)
        .unwrap()
}

pub fn authenticated_json_request(uri: &str, body: &str) -> Request<Body> {
    let mut request = authenticated_request(Method::POST, uri, Body::from(body.to_string()));
    request.headers_mut().insert(
        CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    request
}

pub async fn send_json(app: &Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned())
        })
    };
    (status, value)
}

pub fn setup_flow_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_flow(
        dir.path(),
        "hello_world.lua",
        r#"
        local flow = Flow.new("webhook_test")
        flow:step("greet", nodes.log({ message = "hello from webhook" }))
        return flow
        "#,
    );
    dir
}
