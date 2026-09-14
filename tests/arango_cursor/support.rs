use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use tokio::sync::Notify;

#[derive(Clone, Debug)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub headers: HeaderMap,
    pub body: Bytes,
}

pub struct Reply {
    pub status: StatusCode,
    pub body: String,
    pub release: Option<Arc<Notify>>,
}

impl Reply {
    pub fn json(status: StatusCode, body: Value) -> Self {
        Self {
            status,
            body: body.to_string(),
            release: None,
        }
    }

    pub fn page(rows: Value, has_more: bool, cursor: Option<&str>) -> Self {
        let mut body = json!({"result": rows, "hasMore": has_more, "error": false});
        if let Some(cursor) = cursor {
            body["id"] = json!(cursor);
        }
        Self::json(StatusCode::OK, body)
    }

    pub fn closed() -> Self {
        Self::json(
            StatusCode::ACCEPTED,
            json!({"error": false, "id": "123", "code": 202}),
        )
    }
}

#[derive(Clone)]
struct Fixture {
    requests: Arc<Mutex<Vec<Request>>>,
    replies: Arc<Mutex<VecDeque<Reply>>>,
    observed: Arc<Notify>,
}

pub struct Server {
    pub url: String,
    state: Fixture,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    pub async fn start(replies: Vec<Reply>) -> Self {
        let state = Fixture {
            requests: Default::default(),
            replies: Arc::new(Mutex::new(replies.into())),
            observed: Default::default(),
        };
        let app = Router::new().fallback(handle).with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { url, state, task }
    }

    pub fn config(&self) -> Value {
        json!({"url": self.url, "database": "fixture", "token": "synthetic-token", "timeout": 2})
    }

    pub fn requests(&self) -> Vec<Request> {
        self.state.requests.lock().unwrap().clone()
    }

    pub async fn wait_for_requests(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let observed = self.state.observed.notified();
                if self.requests().len() >= count {
                    break;
                }
                observed.await;
            }
        })
        .await
        .expect("expected Cursor API request did not arrive");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle(
    State(state): State<Fixture>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    state.requests.lock().unwrap().push(Request {
        method,
        path: uri.path().to_string(),
        headers,
        body,
    });
    let reply = state.replies.lock().unwrap().pop_front();
    state.observed.notify_one();
    let Some(reply) = reply else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "unexpected extra request",
        )
            .into_response();
    };
    if let Some(release) = reply.release {
        release.notified().await;
    }
    (
        reply.status,
        [("content-type", "application/json")],
        reply.body,
    )
        .into_response()
}
