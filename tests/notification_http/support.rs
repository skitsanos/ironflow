use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::Response;
use futures_util::{StreamExt, stream};
use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};
use tokio::sync::Notify;

pub const NODES: [&str; 3] = ["http_get", "slack_notification", "send_email"];

pub struct Server {
    pub url: String,
    pub started: std::sync::Arc<Notify>,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    pub async fn start(
        status: u16,
        content_type: &str,
        chunks: Vec<Vec<u8>>,
        declared: Option<u64>,
        stall: bool,
        broken: bool,
    ) -> Self {
        let started = std::sync::Arc::new(Notify::new());
        let signal = started.clone();
        let content_type = content_type.to_owned();
        let app = Router::new().fallback(move || {
            let chunks = chunks.clone();
            let content_type = content_type.clone();
            let signal = signal.clone();
            async move {
                let body = stream::iter(
                    chunks
                        .into_iter()
                        .map(|chunk| Ok::<_, std::io::Error>(Bytes::from(chunk))),
                )
                .chain(stream::once(async move {
                    signal.notify_one();
                    if stall {
                        std::future::pending::<()>().await;
                    }
                    // Delay the transport error so the valid prefix reaches the reader.
                    if broken {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Err(std::io::Error::other("synthetic stream failure"))
                    } else {
                        Ok(Bytes::new())
                    }
                }));
                let mut response = Response::builder()
                    .status(status)
                    .header("content-type", content_type);
                if let Some(length) = declared {
                    response = response.header("content-length", length);
                }
                response.body(Body::from_stream(body)).unwrap()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/services/path-sentinel?token=query-sentinel",
            listener.local_addr().unwrap()
        );
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { url, started, task }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub fn config(node: &str, url: &str) -> Value {
    match node {
        "http_get" => {
            json!({"url": url, "timeout": 5, "output_key": "result", "fail_on_status": false})
        }
        "slack_notification" => json!({
            "webhook_url": url, "text": "fixture", "timeout": 5, "output_key": "result"
        }),
        "send_email" => json!({
            "api_url": url, "api_key": "synthetic-api-key", "to": "to@example.invalid",
            "from": "from@example.invalid", "subject": "fixture", "text": "fixture",
            "timeout": 5, "output_key": "result"
        }),
        _ => panic!("unexpected node"),
    }
}

pub async fn execute(node: &str, config: &Value) -> anyhow::Result<NodeOutput> {
    tokio::time::timeout(
        Duration::from_secs(10),
        NodeRegistry::with_builtins()
            .get(node)
            .unwrap()
            .execute(config, &Context::new()),
    )
    .await
    .expect("node did not finish")
}

pub fn assert_redacted(error: &anyhow::Error) {
    let message = format!("{error:#}");
    for secret in [
        "path-sentinel",
        "query-sentinel",
        "body-sentinel",
        "synthetic-api-key",
    ] {
        assert!(
            !message.contains(secret),
            "error disclosed {secret}: {message}"
        );
    }
}

// Give each case a fresh environment before starting its runtime, without
// racing process-global setenv against other tests or HTTP client threads.
pub async fn isolated(name: &str) -> bool {
    if std::env::var("IRONFLOW_NOTIFICATION_CASE").as_deref() == Ok(name) {
        return false;
    }
    let directory = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg(name)
        .arg("--nocapture")
        .env_clear()
        .env("IRONFLOW_NOTIFICATION_CASE", name)
        .env("IRONFLOW_MAX_HTTP_BODY_BYTES", "1024")
        .current_dir(directory.path())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .expect("isolated notification case timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}
