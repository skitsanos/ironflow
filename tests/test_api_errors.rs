use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get};
use http_body_util::BodyExt;
use ironflow::api::errors::{AppError, ERROR_ID_HEADER};
use ironflow::engine::types::{Context, RunInfo, RunStatus, TaskState};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::{RunListQuery, RunSummaryPage, StateStore, StorageError, StorageResult};
use tower::ServiceExt;

struct FailingStateStore {
    error: StorageError,
    get_run_info_calls: AtomicUsize,
    delete_run_calls: AtomicUsize,
}

impl FailingStateStore {
    fn new(error: StorageError) -> Self {
        Self {
            error,
            get_run_info_calls: AtomicUsize::new(0),
            delete_run_calls: AtomicUsize::new(0),
        }
    }

    fn error(&self) -> StorageError {
        self.error.clone()
    }
}

#[async_trait]
impl StateStore for FailingStateStore {
    async fn init_run(&self, _run_id: &str, _flow_name: &str, _ctx: &Context) -> StorageResult<()> {
        Err(self.error())
    }

    async fn set_run_status(&self, _run_id: &str, _status: RunStatus) -> StorageResult<()> {
        Err(self.error())
    }

    async fn upsert_task(&self, _run_id: &str, _task: &TaskState) -> StorageResult<()> {
        Err(self.error())
    }

    async fn get_ctx(&self, _run_id: &str) -> StorageResult<Context> {
        Err(self.error())
    }

    async fn update_ctx(&self, _run_id: &str, _ctx: &Context) -> StorageResult<()> {
        Err(self.error())
    }

    async fn get_run_info(&self, _run_id: &str) -> StorageResult<RunInfo> {
        self.get_run_info_calls.fetch_add(1, Ordering::Relaxed);
        Err(self.error())
    }

    async fn list_runs(&self, _status: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        Err(self.error())
    }

    async fn list_run_summaries_page(
        &self,
        _query: &RunListQuery,
    ) -> StorageResult<RunSummaryPage> {
        Err(self.error())
    }

    async fn delete_run(&self, _run_id: &str) -> StorageResult<()> {
        self.delete_run_calls.fetch_add(1, Ordering::Relaxed);
        Err(self.error())
    }
}

fn error_contract_app(store: Arc<dyn StateStore>) -> Router {
    let state = Arc::new(ironflow::api::AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store,
        event_store: Arc::new(MemoryEventStore::new()),
        flows_dir: None,
        max_concurrent_tasks: None,
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: std::collections::HashMap::new(),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
        metrics: None,
    });

    Router::new()
        .route("/runs/{id}", get(ironflow::api::handlers::get_run))
        .route("/runs/{id}", delete(ironflow::api::handlers::delete_run))
        .route(
            "/runs/{id}/events",
            get(ironflow::api::handlers::run_events),
        )
        .with_state(state)
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn internal_errors_are_generic_correlated_and_unique() {
    const SENTINEL: &str = "do-not-disclose-this-password";
    let first = AppError::Internal(anyhow::anyhow!(
        "connect failed at postgres://operator:{SENTINEL}@db.example.test/ironflow"
    ))
    .into_response();
    let second = AppError::Internal(anyhow::anyhow!("second internal failure")).into_response();

    assert_eq!(first.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let first_header = first.headers()[ERROR_ID_HEADER]
        .to_str()
        .unwrap()
        .to_string();
    let second_header = second.headers()[ERROR_ID_HEADER]
        .to_str()
        .unwrap()
        .to_string();
    uuid::Uuid::parse_str(&first_header).expect("error ID must be a UUID");
    uuid::Uuid::parse_str(&second_header).expect("error ID must be a UUID");
    assert_ne!(first_header, second_header);

    let first_body = response_json(first).await;
    let second_body = response_json(second).await;
    assert_eq!(first_body["error"], "Internal server error");
    assert_eq!(first_body["code"], "internal_error");
    assert_eq!(first_body["error_id"].as_str(), Some(first_header.as_str()));
    assert_eq!(
        second_body["error_id"].as_str(),
        Some(second_header.as_str())
    );
    assert!(first_body.get("details").is_none());
    assert!(!first_body.to_string().contains(SENTINEL));
}

#[tokio::test]
async fn typed_storage_errors_map_to_run_api_statuses() {
    let missing = Arc::new(FailingStateStore::new(StorageError::not_found(
        "Run 'missing' not found",
    )));
    let response = error_contract_app(missing)
        .oneshot(
            Request::builder()
                .uri("/runs/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["code"], "not_found");
    assert!(body.get("error_id").is_none());

    let backend = Arc::new(FailingStateStore::new(StorageError::backend(
        "read run",
        "database unavailable",
    )));
    let response = error_contract_app(backend)
        .oneshot(
            Request::builder()
                .uri("/runs/backend-failure")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = response_json(response).await;
    assert_eq!(body["error"], "Internal server error");
    assert_eq!(body["code"], "internal_error");
    uuid::Uuid::parse_str(body["error_id"].as_str().unwrap()).unwrap();
}

#[tokio::test]
async fn delete_uses_typed_conflict_without_a_read_preflight() {
    let store = Arc::new(FailingStateStore::new(StorageError::conflict(
        "Run changed while it was being deleted",
    )));
    let response = error_contract_app(store.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/runs/changing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(store.get_run_info_calls.load(Ordering::Relaxed), 0);
    assert_eq!(store.delete_run_calls.load(Ordering::Relaxed), 1);
    let body = response_json(response).await;
    assert_eq!(body["code"], "conflict");
}

#[tokio::test]
async fn sse_preflight_does_not_report_corruption_as_not_found() {
    let store = Arc::new(FailingStateStore::new(StorageError::corruption(
        "read run",
        "stored run JSON is invalid",
    )));
    let response = error_contract_app(store)
        .oneshot(
            Request::builder()
                .uri("/runs/corrupt/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = response_json(response).await;
    assert_eq!(body["code"], "internal_error");
}
