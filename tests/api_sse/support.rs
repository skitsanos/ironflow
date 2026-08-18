use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderValue, Request};
use axum::routing::get;
use http_body_util::BodyExt;
use ironflow::engine::types::{Context, RunStatus};
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::EventStore;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::storage::{StateStore, StorageResult};
use tokio::sync::Mutex;
use tower::ServiceExt;

pub const RUN_ID: &str = "sse-contract-run";

pub struct Harness {
    _dir: tempfile::TempDir,
    router: Router,
    pub store: Arc<JsonStateStore>,
}

impl Harness {
    pub async fn request(
        &self,
        uri: &str,
        last_event_ids: &[HeaderValue],
    ) -> axum::response::Response {
        let mut request = Request::builder().uri(uri).body(Body::empty()).unwrap();
        for value in last_event_ids {
            request.headers_mut().append("last-event-id", value.clone());
        }
        self.router.clone().oneshot(request).await.unwrap()
    }
}

pub struct ScriptedEventStore {
    responses: Mutex<VecDeque<StorageResult<Vec<RunEvent>>>>,
    cursors: Mutex<Vec<Option<String>>>,
}

impl ScriptedEventStore {
    pub fn new(responses: Vec<StorageResult<Vec<RunEvent>>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            cursors: Mutex::new(Vec::new()),
        }
    }

    pub async fn cursors(&self) -> Vec<Option<String>> {
        self.cursors.lock().await.clone()
    }
}

#[async_trait]
impl EventStore for ScriptedEventStore {
    async fn publish(&self, _event: RunEvent) -> StorageResult<()> {
        Ok(())
    }

    async fn delete_run(&self, _run_id: &str) -> StorageResult<usize> {
        Ok(0)
    }

    async fn list_since(
        &self,
        _run_id: &str,
        after: Option<&str>,
        _limit: usize,
    ) -> StorageResult<Vec<RunEvent>> {
        self.cursors.lock().await.push(after.map(str::to_string));
        self.responses
            .lock()
            .await
            .pop_front()
            .unwrap_or_else(|| Ok(Vec::new()))
    }
}

pub async fn harness(event_store: Arc<dyn EventStore>, status: RunStatus) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(dir.path()));
    store
        .init_run(RUN_ID, "sse_contract", &Context::new())
        .await
        .unwrap();
    store.set_run_status(RUN_ID, status).await.unwrap();
    let state = Arc::new(ironflow::api::AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store: store.clone(),
        event_store,
        flows_dir: None,
        max_concurrent_tasks: None,
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: std::collections::HashMap::new(),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
        metrics: None,
    });
    let router = Router::new()
        .route(
            "/runs/{id}/events",
            get(ironflow::api::handlers::run_events),
        )
        .with_state(state);
    Harness {
        _dir: dir,
        router,
        store,
    }
}

pub fn run_event(id: impl Into<String>, event_type: RunEventType) -> RunEvent {
    let status = if event_type == RunEventType::RunFinished {
        RunStatus::Success
    } else {
        RunStatus::Running
    };
    let mut event = RunEvent::run(RUN_ID, "sse_contract", event_type, status);
    event.id = id.into();
    event
}

pub async fn response_text(response: axum::response::Response) -> String {
    let collected = tokio::time::timeout(Duration::from_secs(6), response.into_body().collect())
        .await
        .expect("SSE response should terminate")
        .expect("SSE body should be valid");
    String::from_utf8(collected.to_bytes().to_vec()).unwrap()
}

pub async fn response_json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_str(&response_text(response).await).unwrap()
}

pub fn event_ids(body: &str) -> Vec<&str> {
    body.lines()
        .filter_map(|line| line.strip_prefix("id: "))
        .collect()
}

pub fn frames(body: &str) -> Vec<&str> {
    body.split("\n\n")
        .filter(|frame| !frame.is_empty())
        .collect()
}

pub fn frame_json(frame: &str) -> serde_json::Value {
    let data = frame
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("SSE frame should contain data");
    serde_json::from_str(data).unwrap()
}
