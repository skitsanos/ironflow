use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::Response;
use futures_util::stream;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

struct DownloadServer {
    endpoint: String,
    sent: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl DownloadServer {
    async fn start(bytes: Vec<u8>, delay: Duration) -> Self {
        let bytes = Arc::new(bytes);
        let sent = Arc::new(AtomicUsize::new(0));
        let count = sent.clone();
        let app = Router::new().fallback(move || {
            let bytes = bytes.clone();
            let count = count.clone();
            async move {
                let length = bytes.len();
                let body = stream::unfold(
                    (0, bytes, count),
                    move |(offset, bytes, count)| async move {
                        if offset == bytes.len() {
                            return None;
                        }
                        tokio::time::sleep(delay).await;
                        let end = (offset + 1024).min(bytes.len());
                        let chunk = Bytes::copy_from_slice(&bytes[offset..end]);
                        count.fetch_add(chunk.len(), Ordering::AcqRel);
                        Some((Ok::<_, std::io::Error>(chunk), (end, bytes, count)))
                    },
                );
                Response::builder()
                    .status(200)
                    .header("content-length", length)
                    .body(Body::from_stream(body))
                    .unwrap()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            endpoint,
            sent,
            task,
        }
    }
}

impl Drop for DownloadServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn replica_consumer_cli_restores_or_cancels_without_partial_publication() {
    let flow = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/22-replica-deployment/artifact_consume.lua");
    let bytes = vec![b'x'; 128 * 1024];
    let digest = hex::encode(Sha256::digest(&bytes));
    let uri = format!("artifact://sha256/{digest}");
    for (action, cancel, uri_only) in [
        ("validate", false, false),
        ("run", false, false),
        ("run", false, true),
        ("run", true, false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        let destination = directory.path().join("restored.bin");
        std::fs::write(&destination, b"existing output").unwrap();
        let delay = Duration::from_millis(if cancel { 20 } else { 1 });
        let server = DownloadServer::start(bytes.clone(), delay).await;
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
        command
            .env_clear()
            .current_dir(directory.path())
            .kill_on_drop(true)
            .env("IRONFLOW_ARTIFACT_BACKEND", "s3")
            .env("IRONFLOW_ARTIFACT_DIR", &cache)
            .env("IRONFLOW_ARTIFACT_S3_BUCKET", "fixture")
            .env("IRONFLOW_ARTIFACT_S3_ENDPOINT_URL", &server.endpoint)
            .env("IRONFLOW_ARTIFACT_S3_REGION", "us-east-1")
            .env("IRONFLOW_ARTIFACT_S3_FORCE_PATH_STYLE", "true")
            .env("AWS_ACCESS_KEY_ID", "fixture")
            .env("AWS_SECRET_ACCESS_KEY", "fixture")
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .env("AWS_CONFIG_FILE", directory.path().join("no-config"))
            .env(
                "AWS_SHARED_CREDENTIALS_FILE",
                directory.path().join("no-credentials"),
            )
            .arg(action)
            .arg(&flow);
        if action == "run" {
            let artifact = if uri_only {
                json!(uri)
            } else {
                json!({
                    "artifact_uri": uri, "sha256": digest, "size_bytes": bytes.len(),
                    "mime_type": "application/octet-stream"
                })
            };
            command
                .arg("--store-dir")
                .arg(directory.path().join("state"))
                .arg("--context")
                .arg(json!({"artifact": artifact, "output_path": destination}).to_string());
        }
        if cancel {
            command.env("IRONFLOW_MAX_RUN_SECONDS", "1");
        }
        let output = tokio::time::timeout(Duration::from_secs(15), command.output())
            .await
            .expect("artifact CLI did not settle")
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), !cancel, "{stdout}\n{stderr}");
        if action == "validate" {
            continue;
        }
        let (_, encoded) = stdout
            .split_once("\nContext:\n")
            .expect("CLI context missing");
        let context: Value = serde_json::from_str(encoded.trim()).unwrap();
        if cancel {
            assert!(stdout.contains("Status: cancelled"), "{stdout}");
            assert!(context.get("write_file_success").is_none());
            assert!(context.get("write_file_path").is_none());
            assert!(
                server.sent.load(Ordering::Acquire) > 1024,
                "run did not reach a progressing download"
            );
            assert!(
                server.sent.load(Ordering::Acquire) < bytes.len(),
                "cancelled CLI consumed the entire object"
            );
            assert_eq!(std::fs::read(&destination).unwrap(), b"existing output");
            assert_eq!(std::fs::read_dir(cache.join("sha256")).unwrap().count(), 0);
        } else {
            assert_eq!(context["write_file_success"], true);
            assert_eq!(std::fs::read(&destination).unwrap(), bytes);
            assert_eq!(
                std::fs::read(cache.join("sha256").join(&digest)).unwrap(),
                bytes
            );
            assert_eq!(std::fs::read_dir(cache.join("sha256")).unwrap().count(), 1);
        }
        let mut names: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, ["cache", "restored.bin", "state"]);
    }
}

#[tokio::test]
async fn artifact_handoff_example_validates() {
    let directory = tempfile::tempdir().unwrap();
    let flow = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/04-file-operations/artifact_handoff.lua");
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"))
            .env_clear()
            .current_dir(directory.path())
            .kill_on_drop(true)
            .arg("validate")
            .arg(flow)
            .output(),
    )
    .await
    .expect("example validation timed out")
    .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
