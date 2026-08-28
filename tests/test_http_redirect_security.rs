//! Cross-origin redirect policy for HTTP nodes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{any, get};
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

struct TestServer {
    url: String,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn spawn(app: Router) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    TestServer {
        url: format!("http://{address}"),
        task,
    }
}

/// Redirect from HTTP to HTTPS on the same explicit host and port. Reqwest's
/// own credential stripping compares only host and port, so URL userinfo would
/// survive this origin change unless IronFlow's policy fences it first.
async fn spawn_same_port_scheme_redirect() -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/start", get(redirect))
        .with_state(format!("https://{address}/capture"));
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    TestServer {
        url: format!("http://{address}"),
        task,
    }
}

#[derive(Default)]
struct Capture {
    hits: AtomicUsize,
    referer: Mutex<Option<String>>,
    method: Mutex<Option<Method>>,
    content_type: Mutex<Option<String>>,
    body: Mutex<Vec<u8>>,
}

async fn target(
    State(capture): State<Arc<Capture>>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> &'static str {
    capture.hits.fetch_add(1, Ordering::SeqCst);
    *capture.method.lock().unwrap() = Some(method);
    *capture.referer.lock().unwrap() = headers
        .get(header::REFERER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    *capture.content_type.lock().unwrap() = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    *capture.body.lock().unwrap() = body.to_vec();
    "ok"
}

async fn redirect(State(location): State<String>) -> impl IntoResponse {
    (StatusCode::FOUND, [(header::LOCATION, location)])
}

async fn temporary_redirect(State(location): State<String>) -> impl IntoResponse {
    (
        StatusCode::TEMPORARY_REDIRECT,
        [(header::LOCATION, location)],
    )
}

async fn relative_found_redirect() -> impl IntoResponse {
    (StatusCode::FOUND, [(header::LOCATION, "/capture")])
}

async fn relative_temporary_redirect() -> impl IntoResponse {
    (
        StatusCode::TEMPORARY_REDIRECT,
        [(header::LOCATION, "/capture")],
    )
}

async fn execute(config: serde_json::Value) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
    execute_node("http_get", config).await
}

async fn execute_node(
    node_type: &str,
    config: serde_json::Value,
) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
    NodeRegistry::with_builtins()
        .get(node_type)
        .unwrap()
        .execute(&config, &Context::new())
        .await
}

async fn redirect_pair() -> (TestServer, TestServer, Arc<Capture>) {
    let capture = Arc::new(Capture::default());
    let destination = spawn(
        Router::new()
            .route("/capture", any(target))
            .with_state(capture.clone()),
    )
    .await;
    let origin = spawn(
        Router::new()
            .route("/start", get(redirect))
            .route("/temporary", any(temporary_redirect))
            .with_state(format!("{}/capture", destination.url)),
    )
    .await;
    (origin, destination, capture)
}

#[tokio::test]
async fn api_key_cannot_cross_an_origin_even_with_cross_origin_opt_in() {
    let (origin, _destination, capture) = redirect_pair().await;
    let error = execute(serde_json::json!({
        "url": format!("{}/start", origin.url),
        "allow_cross_origin_redirects": true,
        "auth": {
            "type": "api_key",
            "header": "X-API-Key",
            "key": "must-not-leave-origin"
        }
    }))
    .await
    .unwrap_err()
    .to_string();

    assert!(
        error.contains("configured auth, headers, or a body"),
        "{error}"
    );
    assert_eq!(capture.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn caller_header_cannot_cross_an_origin_even_with_cross_origin_opt_in() {
    let (origin, _destination, capture) = redirect_pair().await;
    let error = execute(serde_json::json!({
        "url": format!("{}/start", origin.url),
        "allow_cross_origin_redirects": true,
        "headers": { "X-Workflow-Credential": "must-not-leave-origin" }
    }))
    .await
    .unwrap_err()
    .to_string();

    assert!(
        error.contains("configured auth, headers, or a body"),
        "{error}"
    );
    assert_eq!(capture.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn url_userinfo_cannot_cross_a_scheme_origin_even_with_opt_in() {
    let origin = spawn_same_port_scheme_redirect().await;
    let credential_url =
        origin
            .url
            .replacen("http://", "http://workflow-user:workflow-password@", 1);
    let error = execute(serde_json::json!({
        "url": format!("{credential_url}/start"),
        "allow_cross_origin_redirects": true,
    }))
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("including URL credentials"), "{error}");
    assert!(!error.contains("workflow-user"), "{error}");
    assert!(!error.contains("workflow-password"), "{error}");
}

#[tokio::test]
async fn plain_cross_origin_redirect_requires_explicit_opt_in() {
    let (origin, _destination, capture) = redirect_pair().await;
    let error = execute(serde_json::json!({
        "url": format!("{}/start", origin.url),
    }))
    .await
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("cross-origin redirects are disabled"),
        "{error}"
    );
    assert_eq!(capture.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn plain_cross_origin_redirect_can_be_explicitly_enabled() {
    let (origin, _destination, capture) = redirect_pair().await;
    let output = execute(serde_json::json!({
        "url": format!("{}/start", origin.url),
        "allow_cross_origin_redirects": true,
        "max_redirects": 1,
    }))
    .await
    .unwrap();

    assert_eq!(output["http_status"], 200);
    assert_eq!(capture.hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cross_origin_opt_in_does_not_generate_a_query_bearing_referer() {
    let (origin, _destination, capture) = redirect_pair().await;
    execute(serde_json::json!({
        "url": format!("{}/start?api_key=query-secret", origin.url),
        "allow_cross_origin_redirects": true,
    }))
    .await
    .unwrap();

    assert_eq!(capture.hits.load(Ordering::SeqCst), 1);
    assert_eq!(*capture.referer.lock().unwrap(), None);
}

#[tokio::test]
async fn request_body_cannot_cross_an_origin_even_with_opt_in() {
    let (origin, _destination, capture) = redirect_pair().await;
    let error = execute_node(
        "http_post",
        serde_json::json!({
            "url": format!("{}/temporary", origin.url),
            "allow_cross_origin_redirects": true,
            "body": { "secret": "must-not-leave-origin" }
        }),
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(
        error.contains("configured auth, headers, or a body"),
        "{error}"
    );
    assert_eq!(capture.hits.load(Ordering::SeqCst), 0);
    assert!(capture.body.lock().unwrap().is_empty());
}

#[tokio::test]
async fn found_redirect_converts_post_to_bodyless_get() {
    let capture = Arc::new(Capture::default());
    let server = spawn(
        Router::new()
            .route("/start", any(relative_found_redirect))
            .route("/capture", any(target))
            .with_state(capture.clone()),
    )
    .await;

    execute_node(
        "http_post",
        serde_json::json!({
            "url": format!("{}/start", server.url),
            "proxy_mode": "direct",
            "max_redirects": 1,
            "body": {"secret": "same-origin"}
        }),
    )
    .await
    .unwrap();

    assert_eq!(*capture.method.lock().unwrap(), Some(Method::GET));
    assert!(capture.body.lock().unwrap().is_empty());
    assert_eq!(*capture.content_type.lock().unwrap(), None);
}

#[tokio::test]
async fn temporary_redirect_replays_post_body_on_same_origin() {
    let capture = Arc::new(Capture::default());
    let server = spawn(
        Router::new()
            .route("/start", any(relative_temporary_redirect))
            .route("/capture", any(target))
            .with_state(capture.clone()),
    )
    .await;

    execute_node(
        "http_post",
        serde_json::json!({
            "url": format!("{}/start", server.url),
            "proxy_mode": "direct",
            "max_redirects": 1,
            "body": {"message": "replay"}
        }),
    )
    .await
    .unwrap();

    assert_eq!(*capture.method.lock().unwrap(), Some(Method::POST));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&capture.body.lock().unwrap()).unwrap(),
        serde_json::json!({"message": "replay"})
    );
    assert_eq!(
        capture.content_type.lock().unwrap().as_deref(),
        Some("application/json")
    );
}

#[tokio::test]
async fn zero_redirect_limit_returns_the_redirect_response() {
    let capture = Arc::new(Capture::default());
    let server = spawn(
        Router::new()
            .route("/start", any(relative_found_redirect))
            .route("/capture", any(target))
            .with_state(capture.clone()),
    )
    .await;

    let output = execute(serde_json::json!({
        "url": format!("{}/start", server.url),
        "proxy_mode": "direct",
        "max_redirects": 0,
        "fail_on_status": false
    }))
    .await
    .unwrap();

    assert_eq!(output["http_status"], 302);
    assert_eq!(capture.hits.load(Ordering::SeqCst), 0);
}
