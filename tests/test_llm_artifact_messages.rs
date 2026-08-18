use std::path::Path;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::routing::post;
use ironflow::artifacts::{ArtifactRef, LocalArtifactStore};
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use ironflow::util::execution::with_execution_deadline;
use serde_json::Value;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct ArtifactEnvironment(Option<std::ffi::OsString>);

impl ArtifactEnvironment {
    fn set(path: &Path) -> Self {
        let original = std::env::var_os("IRONFLOW_ARTIFACT_DIR");
        // SAFETY: artifact-environment mutation in this test binary is serialized.
        unsafe { std::env::set_var("IRONFLOW_ARTIFACT_DIR", path) };
        Self(original)
    }
}

impl Drop for ArtifactEnvironment {
    fn drop(&mut self) {
        // SAFETY: ENV_LOCK remains held until this guard drops.
        unsafe {
            match &self.0 {
                Some(value) => std::env::set_var("IRONFLOW_ARTIFACT_DIR", value),
                None => std::env::remove_var("IRONFLOW_ARTIFACT_DIR"),
            }
        }
    }
}

async fn create_png_artifact(directory: &Path) -> Value {
    let source = directory.join("source.png");
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        2,
        image::Rgba([15, 80, 160, 255]),
    ))
    .save(&source)
    .unwrap();
    NodeRegistry::with_builtins()
        .get("read_file")
        .unwrap()
        .execute(
            &serde_json::json!({
                "path": source,
                "encoding": "artifact",
                "mime_type": "image/png",
                "output_key": "image"
            }),
            &Context::new(),
        )
        .await
        .unwrap()["image_artifact"]
        .clone()
}

async fn request_server() -> (String, Arc<tokio::sync::Mutex<Vec<Value>>>) {
    let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let captured = requests.clone();
    let app = Router::new().route(
        "/chat/completions",
        post(move |Json(body): Json<Value>| {
            let captured = captured.clone();
            async move {
                captured.lock().await.push(body);
                Json(serde_json::json!({
                    "choices": [{"message": {"content": "image accepted"}}]
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), requests)
}

async fn echo_server(status: StatusCode) -> String {
    let app = Router::new().route(
        "/chat/completions",
        post(move |Json(body): Json<Value>| async move {
            let url = body["messages"][0]["content"][0]["image_url"]["url"]
                .as_str()
                .unwrap();
            (
                status,
                Json(serde_json::json!({
                    "choices": [{"message": {"content": format!("echo: {url}")}}],
                    "echo": url
                })),
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

fn llm_config(base_url: &str) -> Value {
    serde_json::json!({
        "provider": "custom",
        "mode": "chat",
        "base_url": base_url,
        "auth_type": "none",
        "messages_key": "conversation",
        "output_key": "vision"
    })
}

#[tokio::test]
async fn llm_resolves_descriptor_and_uri_images_from_messages_key() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let _environment = ArtifactEnvironment::set(&directory.path().join("artifacts"));
    let descriptor = create_png_artifact(directory.path()).await;
    let uri = descriptor["artifact_uri"].clone();
    let conversation = serde_json::json!([{
        "role": "user",
        "content": [
            {"type": "text", "text": "Inspect both images"},
            {"type": "image_artifact", "source_key": "descriptor", "detail": "high"},
            {"type": "image_artifact", "source_key": "uri"}
        ]
    }]);
    let context = Context::from([
        ("conversation".to_string(), conversation.clone()),
        ("descriptor".to_string(), descriptor),
        ("uri".to_string(), uri),
    ]);
    let (base_url, requests) = request_server().await;

    let output = NodeRegistry::with_builtins()
        .get("llm")
        .unwrap()
        .execute(&llm_config(&base_url), &context)
        .await
        .unwrap();

    assert_eq!(output["vision_text"], "image accepted");
    assert!(
        !serde_json::to_string(&output)
            .unwrap()
            .contains("data:image")
    );
    assert_eq!(context["conversation"], conversation);
    let request = requests.lock().await.pop().unwrap();
    let content = request["messages"][0]["content"].as_array().unwrap();
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(content[1]["image_url"]["detail"], "high");
    assert!(
        content[1]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    assert!(
        content[2]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    assert!(
        !serde_json::to_string(&request)
            .unwrap()
            .contains("image_artifact")
    );
    assert!(
        !serde_json::to_string(&request)
            .unwrap()
            .contains("source_key")
    );
}

#[tokio::test]
async fn llm_enforces_cumulative_image_bytes_and_count_before_request() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let _environment = ArtifactEnvironment::set(&directory.path().join("artifacts"));
    let descriptor = create_png_artifact(directory.path()).await;
    let size = descriptor["size_bytes"].as_u64().unwrap();
    let context = Context::from([
        (
            "conversation".to_string(),
            serde_json::json!([{
                "role": "user",
                "content": [
                    {"type": "image_artifact", "source_key": "image"},
                    {"type": "image_artifact", "source_key": "image"}
                ]
            }]),
        ),
        ("image".to_string(), descriptor),
    ]);
    let (base_url, requests) = request_server().await;
    let node = NodeRegistry::with_builtins().get("llm").unwrap();

    let mut byte_config = llm_config(&base_url);
    byte_config["max_image_input_bytes"] = Value::from(size * 2 - 1);
    let byte_error = node
        .execute(&byte_config, &context)
        .await
        .unwrap_err()
        .to_string();
    assert!(byte_error.contains("llm image artifacts"), "{byte_error}");

    let mut count_config = llm_config(&base_url);
    count_config["max_image_artifacts"] = Value::from(1);
    let count_error = node
        .execute(&count_config, &context)
        .await
        .unwrap_err()
        .to_string();
    assert!(count_error.contains("max_image_artifacts limit of 1"));
    assert!(requests.lock().await.is_empty());
}

#[tokio::test]
async fn llm_rejects_invalid_artifact_blocks_and_messages_selection() {
    let node = NodeRegistry::with_builtins().get("llm").unwrap();
    let base = serde_json::json!({
        "provider": "custom",
        "mode": "chat",
        "base_url": "http://127.0.0.1:1",
        "auth_type": "none"
    });
    let cases = [
        (
            serde_json::json!([{"role": "user", "content": [{
                "type": "image_artifact", "source_key": "missing"
            }]}]),
            "source_key 'missing' not found",
        ),
        (
            serde_json::json!([{"role": "user", "content": [{
                "type": "image_artifact", "source_key": "bad", "extra": true
            }]}]),
            "unknown field 'extra'",
        ),
    ];
    for (conversation, expected) in cases {
        let mut config = base.clone();
        config["messages_key"] = Value::String("conversation".to_string());
        let context = Context::from([
            ("conversation".to_string(), conversation),
            (
                "bad".to_string(),
                Value::String("/tmp/not-an-artifact".to_string()),
            ),
        ]);
        let error = node
            .execute(&config, &context)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(expected),
            "expected {expected:?}, got {error:?}"
        );
    }

    let mut both = base.clone();
    both["messages"] = serde_json::json!([{"role": "user", "content": "inline"}]);
    both["messages_key"] = Value::String("conversation".to_string());
    let error = node
        .execute(
            &both,
            &Context::from([(
                "conversation".to_string(),
                serde_json::json!([{"role": "user", "content": "runtime"}]),
            )]),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("either 'messages' or 'messages_key'"));

    let mut missing = base.clone();
    missing["messages_key"] = Value::String("absent".to_string());
    let error = node
        .execute(&missing, &Context::new())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("messages_key 'absent' not found"));

    let mut not_array = base.clone();
    not_array["messages_key"] = Value::String("conversation".to_string());
    let error = node
        .execute(
            &not_array,
            &Context::from([(
                "conversation".to_string(),
                Value::String("not an array".to_string()),
            )]),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("messages must be an array"));

    let mut responses = base;
    responses["mode"] = Value::String("responses".to_string());
    responses["messages"] = serde_json::json!([{"role": "user", "content": "hello"}]);
    let error = node
        .execute(&responses, &Context::new())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("supported only in chat mode"));
}

#[tokio::test]
async fn llm_rejects_corrupt_artifact_before_contacting_provider() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_directory = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_directory);
    let descriptor = create_png_artifact(directory.path()).await;
    let artifact: ArtifactRef = serde_json::from_value(descriptor.clone()).unwrap();
    let stored = LocalArtifactStore::new(&artifact_directory)
        .unwrap()
        .resolve(&artifact)
        .unwrap();
    make_writable(&stored);
    std::fs::write(&stored, vec![b'x'; artifact.size_bytes as usize]).unwrap();
    let context = Context::from([
        (
            "conversation".to_string(),
            serde_json::json!([{"role": "user", "content": [{
                "type": "image_artifact", "source_key": "image"
            }]}]),
        ),
        ("image".to_string(), descriptor),
    ]);
    let (base_url, requests) = request_server().await;

    let error = NodeRegistry::with_builtins()
        .get("llm")
        .unwrap()
        .execute(&llm_config(&base_url), &context)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("digest verification"), "{error}");
    assert!(requests.lock().await.is_empty());
}

#[tokio::test]
async fn llm_artifact_resolution_honors_execution_deadline() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let _environment = ArtifactEnvironment::set(&directory.path().join("artifacts"));
    let descriptor = create_png_artifact(directory.path()).await;
    let context = Context::from([
        (
            "conversation".to_string(),
            serde_json::json!([{"role": "user", "content": [{
                "type": "image_artifact", "source_key": "image"
            }]}]),
        ),
        ("image".to_string(), descriptor),
    ]);
    let (base_url, requests) = request_server().await;
    let node = NodeRegistry::with_builtins().get("llm").unwrap();

    let error = with_execution_deadline(
        Some(tokio::time::Instant::now()),
        node.execute(&llm_config(&base_url), &context),
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("deadline exceeded"), "{error}");
    assert!(requests.lock().await.is_empty());
}

#[tokio::test]
async fn llm_redacts_artifact_data_urls_echoed_by_provider() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let _environment = ArtifactEnvironment::set(&directory.path().join("artifacts"));
    let descriptor = create_png_artifact(directory.path()).await;
    let context = Context::from([
        (
            "conversation".to_string(),
            serde_json::json!([{"role": "user", "content": [{
                "type": "image_artifact", "source_key": "image"
            }]}]),
        ),
        ("image".to_string(), descriptor),
    ]);
    let node = NodeRegistry::with_builtins().get("llm").unwrap();

    let output = node
        .execute(&llm_config(&echo_server(StatusCode::OK).await), &context)
        .await
        .unwrap();
    let serialized = serde_json::to_string(&output).unwrap();
    assert!(!serialized.contains("data:image"), "{serialized}");
    assert!(serialized.contains("redacted image data URL"));

    let error = node
        .execute(
            &llm_config(&echo_server(StatusCode::BAD_REQUEST).await),
            &context,
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(!error.contains("data:image"), "{error}");
    assert!(error.contains("redacted image data URL"), "{error}");
}

#[cfg(unix)]
fn make_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn make_writable(path: &Path) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(false);
    std::fs::set_permissions(path, permissions).unwrap();
}
