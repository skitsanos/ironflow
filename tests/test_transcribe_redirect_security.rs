use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Redirect;
use axum::routing::{any, post};
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use tokio::sync::Mutex;

const VTT: &str = "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nRedirected.\n";

#[derive(Debug, Default)]
struct UploadCapture {
    hits: usize,
    api_key: Option<String>,
    authorization: Option<String>,
    body: Vec<u8>,
}

async fn capture_upload(
    State(capture): State<Arc<Mutex<UploadCapture>>>,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, &'static str) {
    let mut capture = capture.lock().await;
    capture.hits += 1;
    capture.api_key = headers
        .get("api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    capture.authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    capture.body = body.to_vec();
    (StatusCode::OK, VTT)
}

async fn cross_origin_redirect(State(destination): State<String>) -> Redirect {
    Redirect::to(&destination)
}

async fn same_origin_redirect() -> Redirect {
    Redirect::to("/redirected-transcription")
}

fn audio_fixture() -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private-audio.mp3");
    std::fs::write(&path, b"private audio payload").unwrap();
    (directory, path.to_str().unwrap().to_owned())
}

#[tokio::test]
async fn azure_cross_origin_redirect_sends_neither_key_nor_upload() {
    let destination_capture = Arc::new(Mutex::new(UploadCapture::default()));
    let destination_app = Router::new()
        .route("/leak", any(capture_upload))
        .with_state(destination_capture.clone());
    let destination_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination_url = format!("http://{}/leak", destination_listener.local_addr().unwrap());
    let destination_task = tokio::spawn(async move {
        axum::serve(destination_listener, destination_app)
            .await
            .unwrap();
    });

    let origin_app = Router::new()
        .route(
            "/openai/deployments/whisper-1/audio/transcriptions",
            post(cross_origin_redirect),
        )
        .with_state(destination_url);
    let origin_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_url = format!("http://{}", origin_listener.local_addr().unwrap());
    let origin_task = tokio::spawn(async move {
        axum::serve(origin_listener, origin_app).await.unwrap();
    });

    let (_directory, path) = audio_fixture();
    let config = serde_json::json!({
        "path": path,
        "provider": "azure",
        "base_url": origin_url,
        "api_key": "azure-secret-key",
        "api_version": "2024-06-01",
        "model": "whisper-1",
        "format": "vtt"
    });
    let error = NodeRegistry::with_builtins()
        .get("transcribe")
        .unwrap()
        .execute(&config, &Context::new())
        .await
        .expect_err("cross-origin provider redirect must fail")
        .to_string();

    assert!(error.contains("transcribe request failed"), "{error}");
    assert!(!error.contains("azure-secret-key"), "{error}");
    let captured = destination_capture.lock().await;
    assert_eq!(captured.hits, 0, "redirect destination received a request");
    assert_eq!(
        captured.api_key, None,
        "redirect destination received Azure key"
    );
    assert!(
        captured.body.is_empty(),
        "redirect destination received audio"
    );

    origin_task.abort();
    destination_task.abort();
}

#[tokio::test]
async fn same_origin_redirect_remains_supported() {
    let capture = Arc::new(Mutex::new(UploadCapture::default()));
    let app = Router::new()
        .route("/audio/transcriptions", post(same_origin_redirect))
        .route("/redirected-transcription", any(capture_upload))
        .with_state(capture.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let (_directory, path) = audio_fixture();
    let config = serde_json::json!({
        "path": path,
        "provider": "openai_compatible",
        "base_url": base_url,
        "api_key": "same-origin-key",
        "model": "whisper-1",
        "format": "vtt"
    });
    let output = NodeRegistry::with_builtins()
        .get("transcribe")
        .unwrap()
        .execute(&config, &Context::new())
        .await
        .unwrap();

    assert_eq!(
        output.get("transcript").and_then(|value| value.as_str()),
        Some(VTT)
    );
    let captured = capture.lock().await;
    assert_eq!(captured.hits, 1);
    assert_eq!(
        captured.authorization.as_deref(),
        Some("Bearer same-origin-key")
    );
    // A 303 converts the redirected request to GET, so no multipart body is
    // replayed. This positive case exists to ensure the policy is same-origin,
    // rather than silently disabling every redirect.
    assert!(captured.body.is_empty());

    task.abort();
}
