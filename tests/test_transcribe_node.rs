use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::routing::post;
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use tokio::sync::Mutex;

const VTT: &str = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nHello there.\n";

#[derive(Default)]
struct Captured {
    authorization: Option<String>,
    hits: usize,
}

async fn handler(
    State(state): State<Arc<Mutex<Captured>>>,
    headers: axum::http::HeaderMap,
    _body: axum::body::Bytes,
) -> (axum::http::StatusCode, String) {
    let mut captured = state.lock().await;
    captured.hits += 1;
    captured.authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    (axum::http::StatusCode::OK, VTT.to_string())
}

/// Build the error body a real provider might send: the offending credential
/// echoed back in verbose error text. This is what makes the leak test able
/// to fail if the node's error path ever forwards provider text unredacted.
async fn error_handler(
    status: axum::http::StatusCode,
    headers: axum::http::HeaderMap,
) -> (axum::http::StatusCode, String) {
    let authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let body = serde_json::json!({
        "error": { "message": format!("Invalid credential: {authorization}") }
    })
    .to_string();
    (status, body)
}

async fn start_server(
    status: axum::http::StatusCode,
) -> (String, Arc<Mutex<Captured>>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(Mutex::new(Captured::default()));
    let app = if status == axum::http::StatusCode::OK {
        Router::new()
            .route("/audio/transcriptions", post(handler))
            .with_state(state.clone())
    } else {
        Router::new()
            .route(
                "/audio/transcriptions",
                post(move |headers: axum::http::HeaderMap| error_handler(status, headers)),
            )
            .with_state(state.clone())
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, state, handle)
}

fn audio_fixture() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clip.mp3");
    std::fs::write(&path, b"fake audio bytes").unwrap();
    let path = path.to_str().unwrap().to_string();
    (dir, path)
}

#[tokio::test]
async fn transcribe_returns_vtt_and_reports_metadata() {
    let (url, captured, handle) = start_server(axum::http::StatusCode::OK).await;
    let (_dir, path) = audio_fixture();

    let node = NodeRegistry::with_builtins().get("transcribe").unwrap();
    let config = serde_json::json!({
        "path": path,
        "provider": "openai_compatible",
        "base_url": url,
        "api_key": "sentinel-key-abc123",
        "model": "whisper-large-v3",
        "format": "vtt"
    });

    let output = node.execute(&config, &Context::new()).await.unwrap();

    assert_eq!(output.get("transcript").unwrap().as_str().unwrap(), VTT);
    assert_eq!(output.get("transcript_format").unwrap(), "vtt");
    assert_eq!(output.get("transcript_model").unwrap(), "whisper-large-v3");
    assert_eq!(output.get("transcript_success").unwrap(), true);

    let captured = captured.lock().await;
    assert_eq!(captured.hits, 1);
    assert_eq!(
        captured.authorization.as_deref(),
        Some("Bearer sentinel-key-abc123")
    );

    handle.abort();
}

#[tokio::test]
async fn transcribe_writes_output_file_when_requested() {
    let (url, _captured, handle) = start_server(axum::http::StatusCode::OK).await;
    let (dir, path) = audio_fixture();
    let out = dir.path().join("clip.vtt");

    let node = NodeRegistry::with_builtins().get("transcribe").unwrap();
    let config = serde_json::json!({
        "path": path,
        "provider": "openai_compatible",
        "base_url": url,
        "api_key": "k",
        "output_file": out.to_str().unwrap()
    });

    let output = node.execute(&config, &Context::new()).await.unwrap();

    assert_eq!(
        output.get("transcript_path").unwrap().as_str().unwrap(),
        out.to_str().unwrap()
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap(), VTT);

    handle.abort();
}

#[tokio::test]
async fn transcribe_surfaces_provider_errors_without_leaking_the_key() {
    // The mock echoes the exact `authorization` header it received into the
    // JSON error body, the same way real providers sometimes include the
    // offending credential in verbose error text. If the node's error path
    // ever forwards provider text unredacted, this assertion will catch it.
    let (url, _captured, handle) = start_server(axum::http::StatusCode::UNAUTHORIZED).await;
    let (_dir, path) = audio_fixture();

    let node = NodeRegistry::with_builtins().get("transcribe").unwrap();
    let config = serde_json::json!({
        "path": path,
        "provider": "openai_compatible",
        "base_url": url,
        "api_key": "sentinel-key-abc123"
    });

    let error = node
        .execute(&config, &Context::new())
        .await
        .expect_err("401 must fail the node")
        .to_string();

    assert!(error.contains("401"), "{error}");
    assert!(error.contains("Invalid credential"), "{error}");
    assert!(
        !error.contains("sentinel-key-abc123"),
        "error disclosed the API key: {error}"
    );
    assert!(
        !error.contains("Bearer sentinel-key-abc123"),
        "error disclosed the full authorization header: {error}"
    );

    handle.abort();
}

#[tokio::test]
async fn transcribe_rejects_an_unsupported_format() {
    let (_dir, path) = audio_fixture();
    let node = NodeRegistry::with_builtins().get("transcribe").unwrap();
    let config = serde_json::json!({ "path": path, "api_key": "k", "format": "mp3" });

    let error = node
        .execute(&config, &Context::new())
        .await
        .expect_err("unsupported format must fail")
        .to_string();
    assert!(error.contains("unsupported format"), "{error}");
}
