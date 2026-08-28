use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Method, Response, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{any, get};
use ironflow::artifacts::{ArtifactRef, LocalArtifactStore};
use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const DOWNLOAD_BYTES: &[u8] = b"\x00\xffIronFlow\x80\n";

struct Environment(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl Environment {
    fn local_artifacts(path: &Path) -> Self {
        Self::set(&[
            ("IRONFLOW_ARTIFACT_DIR", path.as_os_str()),
            ("IRONFLOW_ARTIFACT_BACKEND", std::ffi::OsStr::new("local")),
        ])
    }

    fn set(values: &[(&'static str, &std::ffi::OsStr)]) -> Self {
        let originals = values
            .iter()
            .map(|(name, value)| {
                let original = std::env::var_os(name);
                // SAFETY: environment mutation in this test binary is serialized.
                unsafe { std::env::set_var(name, value) };
                (*name, original)
            })
            .collect();
        Self(originals)
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        // SAFETY: the environment lock remains held until this guard drops.
        unsafe {
            for (name, original) in self.0.iter().rev() {
                match original {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

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

#[derive(Default)]
struct Capture {
    requests: Mutex<Vec<CapturedRequest>>,
}

struct CapturedRequest {
    method: Method,
    content_type: Option<String>,
    body: Vec<u8>,
}

async fn capture(
    State(state): State<Arc<Capture>>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    state.requests.lock().unwrap().push(CapturedRequest {
        method,
        content_type: headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        body: body.to_vec(),
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        "{\"ok\":true}",
    )
}

async fn execute(
    node_type: &str,
    config: serde_json::Value,
    context: &Context,
) -> anyhow::Result<NodeOutput> {
    NodeRegistry::with_builtins()
        .get(node_type)
        .unwrap()
        .execute(&config, context)
        .await
}

async fn create_artifact(path: &Path, bytes: &[u8], mime_type: &str) -> serde_json::Value {
    std::fs::write(path, bytes).unwrap();
    let output = execute(
        "read_file",
        serde_json::json!({
            "path": path,
            "encoding": "artifact",
            "mime_type": mime_type,
            "output_key": "source"
        }),
        &Context::new(),
    )
    .await
    .unwrap();
    output["source_artifact"].clone()
}

fn stored_bytes(artifact_dir: &Path, value: serde_json::Value) -> Vec<u8> {
    let artifact: ArtifactRef = serde_json::from_value(value).unwrap();
    let path = LocalArtifactStore::new(artifact_dir)
        .unwrap()
        .resolve(&artifact)
        .unwrap();
    std::fs::read(path).unwrap()
}

fn published_count(artifact_dir: &Path) -> usize {
    std::fs::read_dir(artifact_dir.join("sha256"))
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[tokio::test]
async fn raw_artifact_upload_preserves_bytes_and_mime_type() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = Environment::local_artifacts(&artifact_dir);
    let payload = b"\x00\xffraw artifact\x80";
    let descriptor = create_artifact(
        &directory.path().join("payload.bin"),
        payload,
        "application/x-ironflow-test",
    )
    .await;
    let context = Context::from([("payload".to_owned(), descriptor)]);
    let captured = Arc::new(Capture::default());
    let server = spawn(
        Router::new()
            .route("/upload", any(capture))
            .with_state(captured.clone()),
    )
    .await;

    let output = execute(
        "http_put",
        serde_json::json!({
            "url": format!("{}/upload", server.url),
            "proxy_mode": "direct",
            "body_type": "artifact",
            "body_key": "payload",
            "output_key": "upload"
        }),
        &context,
    )
    .await
    .unwrap();

    let error = execute(
        "http_put",
        serde_json::json!({
            "url": format!("{}/upload", server.url),
            "proxy_mode": "direct",
            "headers": {"Content-Length": "1"},
            "body_type": "artifact",
            "body_key": "payload"
        }),
        &context,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("manage Content-Length"), "{error}");

    assert_eq!(output["upload_data"], serde_json::json!({"ok": true}));
    let requests = captured.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, Method::PUT);
    assert_eq!(requests[0].body, payload);
    assert_eq!(
        requests[0].content_type.as_deref(),
        Some("application/x-ironflow-test")
    );
}

#[tokio::test]
async fn multipart_upload_combines_text_and_artifact_parts() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = Environment::local_artifacts(&artifact_dir);
    let payload = b"\x89PNG\r\n\x1a\nmock";
    let descriptor =
        create_artifact(&directory.path().join("image.png"), payload, "image/png").await;
    let context = Context::from([("image".to_owned(), descriptor)]);
    let captured = Arc::new(Capture::default());
    let server = spawn(
        Router::new()
            .route("/upload", any(capture))
            .with_state(captured.clone()),
    )
    .await;

    execute(
        "http_post",
        serde_json::json!({
            "url": format!("{}/upload", server.url),
            "proxy_mode": "direct",
            "body_type": "multipart",
            "parts": [
                {"name": "note", "text": "slide image"},
                {"name": "image", "source_key": "image", "filename": "slide.png"}
            ]
        }),
        &context,
    )
    .await
    .unwrap();

    let requests = captured.requests.lock().unwrap();
    let request = &requests[0];
    assert!(
        request
            .content_type
            .as_deref()
            .is_some_and(|value| value.starts_with("multipart/form-data; boundary="))
    );
    assert!(contains(&request.body, b"name=\"note\""));
    assert!(contains(&request.body, b"slide image"));
    assert!(contains(&request.body, b"filename=\"slide.png\""));
    assert!(contains(&request.body, b"Content-Type: image/png"));
    assert!(contains(&request.body, payload));
}

async fn download() -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ironflow-binary")
        .body(Body::from(DOWNLOAD_BYTES))
        .unwrap()
}

#[tokio::test]
async fn artifact_response_preserves_binary_without_inline_data() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = Environment::local_artifacts(&artifact_dir);
    let server = spawn(Router::new().route("/download", get(download))).await;

    let output = execute(
        "http_get",
        serde_json::json!({
            "url": format!("{}/download", server.url),
            "proxy_mode": "direct",
            "response_mode": "artifact",
            "output_key": "download"
        }),
        &Context::new(),
    )
    .await
    .unwrap();

    assert!(!output.contains_key("download_data"));
    assert_eq!(output["download_status"], 200);
    assert_eq!(output["download_attempts"], 1);
    assert_eq!(
        output["download_artifact"]["mime_type"],
        "application/x-ironflow-binary"
    );
    assert_eq!(
        stored_bytes(&artifact_dir, output["download_artifact"].clone()),
        DOWNLOAD_BYTES
    );
}

#[derive(Default)]
struct RetryCapture {
    hits: AtomicUsize,
    bodies: Mutex<Vec<Vec<u8>>>,
}

async fn retry_transfer(State(state): State<Arc<RetryCapture>>, body: Bytes) -> Response<Body> {
    state.bodies.lock().unwrap().push(body.to_vec());
    if state.hits.fetch_add(1, Ordering::SeqCst) == 0 {
        Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .header(header::RETRY_AFTER, "0.01")
            .body(Body::from("discarded retry body"))
            .unwrap()
    } else {
        download().await
    }
}

#[tokio::test]
async fn artifact_request_replays_on_status_retry_and_only_final_response_is_stored() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = Environment::local_artifacts(&artifact_dir);
    let payload = b"replayable request artifact";
    let descriptor = create_artifact(
        &directory.path().join("request.bin"),
        payload,
        "application/octet-stream",
    )
    .await;
    let context = Context::from([("request".to_owned(), descriptor)]);
    let captured = Arc::new(RetryCapture::default());
    let server = spawn(
        Router::new()
            .route("/transfer", any(retry_transfer))
            .with_state(captured.clone()),
    )
    .await;

    let output = execute(
        "http_put",
        serde_json::json!({
            "url": format!("{}/transfer", server.url),
            "proxy_mode": "direct",
            "body_type": "artifact",
            "body_key": "request",
            "response_mode": "artifact",
            "retry_statuses": [503],
            "status_retries": 1,
            "status_retry_backoff": 0.01,
            "max_retry_after": 0.01
        }),
        &context,
    )
    .await
    .unwrap();

    assert_eq!(output["http_attempts"], 2);
    assert_eq!(captured.hits.load(Ordering::SeqCst), 2);
    assert_eq!(
        captured.bodies.lock().unwrap().as_slice(),
        [payload, payload]
    );
    assert_eq!(
        stored_bytes(&artifact_dir, output["http_artifact"].clone()),
        DOWNLOAD_BYTES
    );
    assert_eq!(published_count(&artifact_dir), 2);
}

fn spawn_truncated_response() -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 4\r\nConnection: close\r\n\r\nxx",
            )
            .unwrap();
    });
    (format!("http://{address}"), task)
}

fn spawn_stalled_response() -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 4\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        stream.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = stream.write_all(b"late");
    });
    (format!("http://{address}"), task)
}

#[tokio::test]
async fn failed_or_oversized_response_does_not_publish_an_artifact() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = Environment::set(&[
        ("IRONFLOW_ARTIFACT_DIR", artifact_dir.as_os_str()),
        ("IRONFLOW_ARTIFACT_BACKEND", std::ffi::OsStr::new("local")),
        ("IRONFLOW_MAX_HTTP_BODY_BYTES", std::ffi::OsStr::new("4")),
    ]);
    let oversized =
        spawn(Router::new().route("/oversized", get(|| async { Body::from(vec![0_u8; 5]) }))).await;

    let error = execute(
        "http_get",
        serde_json::json!({
            "url": format!("{}/oversized", oversized.url),
            "proxy_mode": "direct",
            "response_mode": "artifact"
        }),
        &Context::new(),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("exceeds limit 4"), "{error}");
    assert_eq!(published_count(&artifact_dir), 0);

    let (url, task) = spawn_truncated_response();
    let error = execute(
        "http_get",
        serde_json::json!({
            "url": url,
            "proxy_mode": "direct",
            "response_mode": "artifact"
        }),
        &Context::new(),
    )
    .await
    .unwrap_err()
    .to_string();
    task.join().unwrap();
    assert!(error.contains("Failed to read HTTP response"), "{error}");
    assert_eq!(published_count(&artifact_dir), 0);

    let (url, task) = spawn_stalled_response();
    let error = execute(
        "http_get",
        serde_json::json!({
            "url": url,
            "proxy_mode": "direct",
            "response_mode": "artifact",
            "timeout": 0.02
        }),
        &Context::new(),
    )
    .await
    .unwrap_err()
    .to_string();
    task.join().unwrap();
    assert!(error.contains("timed out after 0.02 seconds"), "{error}");
    assert_eq!(published_count(&artifact_dir), 0);
}

#[tokio::test]
async fn artifact_upload_respects_the_http_body_limit_before_connecting() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = Environment::local_artifacts(&artifact_dir);
    let descriptor = create_artifact(
        &directory.path().join("request.bin"),
        b"five!",
        "application/octet-stream",
    )
    .await;
    let _limit = Environment::set(&[("IRONFLOW_MAX_HTTP_BODY_BYTES", std::ffi::OsStr::new("4"))]);
    let context = Context::from([("request".to_owned(), descriptor)]);

    let error = execute(
        "http_put",
        serde_json::json!({
            "url": "http://127.0.0.1:1/upload",
            "proxy_mode": "direct",
            "body_type": "artifact",
            "body_key": "request"
        }),
        &context,
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(
        error.contains("exceeds IRONFLOW_MAX_HTTP_BODY_BYTES (4)"),
        "{error}"
    );

    let error = execute(
        "http_post",
        serde_json::json!({
            "url": "http://127.0.0.1:1/upload",
            "proxy_mode": "direct",
            "body_type": "multipart",
            "parts": [{"name": "note", "text": "x"}]
        }),
        &Context::new(),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("multipart payload exceeds"), "{error}");
}
