use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::post};
use serde_json::{Value, json};

#[derive(Clone, Default)]
pub struct Fixture {
    pub calls: Arc<Mutex<Vec<Value>>>,
    pub token_calls: Arc<Mutex<usize>>,
    pub fail_call: Option<usize>,
    pub status: Option<StatusCode>,
    pub topics: bool,
    pub reply: Option<Value>,
    pub reply_call: Option<usize>,
    pub retry_after: Option<&'static str>,
    pub slow: bool,
    pub streamed: bool,
}

pub struct Server {
    pub base: String,
    pub state: Fixture,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    pub async fn start(state: Fixture) -> Self {
        let app = Router::new()
            .route("/embeddings", post(embed))
            .route("/api/embed", post(embed))
            .route("/token", post(token))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { base, state, task }
    }

    pub fn config(&self, provider: &str) -> Value {
        json!({"provider": provider, "base_url": self.base, "ollama_host": self.base,
            "token_url": format!("{}/token", self.base), "client_id": "fixture",
            "client_secret": "synthetic-fixture", "api_key": "synthetic-fixture",
            "input_key": "texts", "source_key": "text"})
    }

    pub fn calls(&self) -> Vec<Value> {
        self.state.calls.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn token(State(state): State<Fixture>) -> Json<Value> {
    *state.token_calls.lock().unwrap() += 1;
    Json(json!({"access_token": "synthetic-fixture", "expires_in": 3600}))
}

async fn embed(State(state): State<Fixture>, uri: Uri, Json(body): Json<Value>) -> Response {
    let call = {
        let mut calls = state.calls.lock().unwrap();
        calls.push(body.clone());
        calls.len()
    };
    let input = body["input"].as_array().unwrap();
    if input.len() > 2048 {
        return (StatusCode::BAD_REQUEST, "array length must be 2048 or less").into_response();
    }
    if let Some(status) = state.status
        && state.fail_call.is_none_or(|index| index == call)
    {
        return (
            status,
            [("retry-after", state.retry_after.unwrap_or("0"))],
            "synthetic-fixture",
        )
            .into_response();
    }
    if state.slow {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
    if let Some(reply) = state.reply
        && state.reply_call.is_none_or(|index| index == call)
    {
        return reply_response(reply, state.streamed);
    }
    let vectors: Vec<_> = input
        .iter()
        .map(|text| {
            let index: usize = text
                .as_str()
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            if state.topics {
                if index < 8 {
                    vec![1.0, 0.0]
                } else {
                    vec![0.0, 1.0]
                }
            } else {
                vec![index as f64, 1.0]
            }
        })
        .collect();
    if uri.path() == "/api/embed" {
        reply_response(json!({"embeddings": vectors}), state.streamed)
    } else {
        // Providers may return indexed vectors out of order within a batch.
        reply_response(
            json!({"data": vectors.iter().enumerate().rev()
            .map(|(index, vector)| json!({"index": index, "embedding": vector})).collect::<Vec<_>>()}),
            state.streamed,
        )
    }
}

fn reply_response(value: Value, streamed: bool) -> Response {
    if !streamed {
        return Json(value).into_response();
    }
    let chunks: Vec<_> = value
        .to_string()
        .as_bytes()
        .chunks(32)
        .map(|chunk| Ok::<_, std::io::Error>(axum::body::Bytes::copy_from_slice(chunk)))
        .collect();
    Response::new(axum::body::Body::from_stream(futures_util::stream::iter(
        chunks,
    )))
}
