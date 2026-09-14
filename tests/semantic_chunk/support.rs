use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::{Json, Router, routing::post};
use serde_json::{Value, json};

#[derive(Clone)]
struct Fixture {
    embeddings: Vec<Vec<f64>>,
    calls: Arc<Mutex<Vec<(Value, HeaderMap)>>>,
}

pub struct Server {
    pub base: String,
    state: Fixture,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    pub async fn start(embeddings: Vec<Vec<f64>>) -> Self {
        let state = Fixture {
            embeddings,
            calls: Default::default(),
        };
        let app = Router::new()
            .route("/embeddings", post(embed))
            .route("/api/embed", post(embed))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { base, state, task }
    }

    pub fn calls(&self) -> Vec<(Value, HeaderMap)> {
        self.state.calls.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn embed(
    State(state): State<Fixture>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    state.calls.lock().unwrap().push((body, headers));
    if uri.path() == "/api/embed" {
        Json(json!({"embeddings": state.embeddings}))
    } else {
        Json(
            json!({"data": state.embeddings.iter().enumerate().map(|(i, e)| json!({"index": i, "embedding": e})).collect::<Vec<_>>() }),
        )
    }
}

pub fn orthogonal_topics() -> Vec<Vec<f64>> {
    (0..16)
        .map(|i| {
            if i < 8 {
                vec![1.0, 0.0]
            } else {
                vec![0.0, 1.0]
            }
        })
        .collect()
}
