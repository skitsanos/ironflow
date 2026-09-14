use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct Captured {
    pub method: Method,
    pub path: String,
    pub headers: HeaderMap,
    pub body: Bytes,
}

#[derive(Clone)]
struct ServerState {
    requests: Arc<Mutex<Vec<Captured>>>,
    redirect: Option<(StatusCode, String)>,
    reply: (StatusCode, Value),
}

pub struct Server {
    pub base: String,
    requests: Arc<Mutex<Vec<Captured>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    pub async fn start(redirect: Option<(StatusCode, String)>, reply: (StatusCode, Value)) -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new().fallback(handle).with_state(ServerState {
            requests: requests.clone(),
            redirect,
            reply,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            base,
            requests,
            task,
        }
    }

    pub fn requests(&self) -> Vec<Captured> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle(
    State(state): State<ServerState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    state.requests.lock().unwrap().push(Captured {
        method,
        path: uri.path().to_string(),
        headers,
        body,
    });
    if uri.path() != "/final"
        && let Some((status, location)) = state.redirect
    {
        return (status, [("location", location)], "redirect").into_response();
    }
    (state.reply.0, Json(state.reply.1)).into_response()
}

pub fn success() -> (StatusCode, Value) {
    (
        StatusCode::OK,
        json!({
            "choices": [{"message": {"content": "accepted"}}],
            "output_text": "accepted",
            "data": [{"embedding": [1.0, 0.0]}, {"embedding": [0.0, 1.0]}],
            "access_token": "synthetic-oauth-token",
            "expires_in": 3600,
            "result": [1],
            "hasMore": false
        }),
    )
}
