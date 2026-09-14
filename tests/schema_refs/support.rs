use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::{Json, Router, routing::get};
use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

pub struct SchemaServer {
    pub url: String,
    requests: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl SchemaServer {
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let app = Router::new().fallback(get({
            let requests = requests.clone();
            move |uri: axum::http::Uri| {
                let requests = requests.clone();
                async move {
                    requests.fetch_add(1, Ordering::SeqCst);
                    if uri.path() == "/slow" {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    Json(json!({"type": "integer"}))
                }
            }
        }));
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            url,
            requests,
            task,
        }
    }

    pub fn assert_unused(&self) {
        assert_eq!(
            self.requests.load(Ordering::SeqCst),
            0,
            "schema retrieval contacted loopback"
        );
    }
}

impl Drop for SchemaServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub async fn execute(
    node_name: &str,
    schema: Value,
    data: Value,
    schema_source: &str,
) -> anyhow::Result<NodeOutput> {
    let node = NodeRegistry::with_builtins().get(node_name).unwrap();
    let data = if node_name == "json_validate" {
        json!(data.to_string())
    } else {
        data
    };
    let mut context = Context::from([("data".into(), data)]);
    let mut config = json!({"source_key": "data"});
    if schema_source == "inline" {
        config["schema"] = schema;
    } else {
        config["schema_key"] = json!("schema");
        context.insert(
            "schema".into(),
            if schema_source == "string" {
                json!(schema.to_string())
            } else {
                schema
            },
        );
    }
    let mut task = tokio::spawn(async move { node.execute(&config, &context).await });
    match tokio::time::timeout(Duration::from_secs(5), &mut task).await {
        Ok(result) => result.expect("schema validation panicked instead of returning an error"),
        Err(_) => {
            task.abort();
            let _ = task.await;
            panic!("schema validation stalled");
        }
    }
}

pub fn assert_reference_error(error: &anyhow::Error) {
    let text = format!("{error:#}");
    assert!(
        text.contains("External schema retrieval is disabled"),
        "{text}"
    );
}
