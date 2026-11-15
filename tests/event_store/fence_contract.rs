//! Deletion-fence contract shared by every event backend (IF-184).
//!
//! The fence a deleted run leaves behind belongs to the incarnation that was
//! deleted: a run accepted again under the same deterministic ID publishes a
//! complete stream, and deleting an ID that never existed fences nothing.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{delete, post};
use http_body_util::BodyExt as _;
use ironflow::api::{AppState, ServiceLifecycle};
use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::EventStore;
use ironflow::storage::sql_store::SqlStateStore;
use ironflow::storage::{StateStore, StorageErrorKind, lifecycle};
use tower::ServiceExt as _;

const FLOW: &str = r#"
    local flow = Flow.new("fence_contract")
    flow:step("first", nodes.log({ message = "one" }))
    flow:step("second", nodes.log({ message = "two" })):depends_on("first")
    return flow
"#;

async fn sqlite_state(directory: &std::path::Path) -> Arc<SqlStateStore> {
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.join("fence-state.sqlite").to_string_lossy()
    );
    Arc::new(SqlStateStore::new(&url).await.unwrap())
}

fn app(store: Arc<dyn StateStore>, events: Arc<dyn EventStore>) -> Router {
    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store,
        event_store: events,
        flows_dir: None,
        max_concurrent_tasks: Some(1),
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: std::collections::HashMap::new(),
        allow_adhoc_flows: true,
        lifecycle: ServiceLifecycle::default(),
        metrics: None,
    });
    Router::new()
        .route("/flows/run", post(ironflow::api::handlers::run_flow))
        .route("/runs/{id}", delete(ironflow::api::handlers::delete_run))
        .with_state(state)
}

/// Flow loading is admitted against a process-wide limit that the other
/// tests of the binary share, so `503` means "retry later" here.
async fn submit(app: &Router, key: &str) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let request = Request::builder()
            .method("POST")
            .uri("/flows/run")
            .header("content-type", "application/json")
            .header("idempotency-key", key)
            .body(Body::from(
                serde_json::json!({ "source": FLOW }).to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        if status == StatusCode::SERVICE_UNAVAILABLE && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
            continue;
        }
        assert!(
            status.is_success(),
            "{status}: {}",
            String::from_utf8_lossy(&body)
        );
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        return body["run_id"].as_str().unwrap().to_string();
    }
}

async fn delete_status(app: &Router, run_id: &str) -> StatusCode {
    let request = Request::builder()
        .method("DELETE")
        .uri(format!("/runs/{run_id}"))
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(request).await.unwrap().status()
}

/// Wait for the terminal event, which the engine publishes after the last
/// task event, and return the whole retained stream.
async fn finished_stream(events: &dyn EventStore, run_id: &str) -> Vec<RunEvent> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let stream = events.list_since(run_id, None, 1_000).await.unwrap();
        if stream
            .iter()
            .any(|event| event.event_type == RunEventType::RunFinished)
        {
            return stream;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "run '{run_id}' retained {} events and no run_finished",
            stream.len()
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_terminal(store: &dyn StateStore, run_id: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while !store
        .get_run_info(run_id)
        .await
        .unwrap()
        .status
        .is_terminal()
    {
        assert!(tokio::time::Instant::now() < deadline, "run did not finish");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn shape(stream: &[RunEvent]) -> Vec<(RunEventType, Option<String>)> {
    stream
        .iter()
        .map(|event| (event.event_type, event.step.clone()))
        .collect()
}

/// `POST` with a key, `DELETE`, the same `POST` again: the second
/// incarnation has the same deterministic ID and a complete stream.
pub async fn assert_resubmitted_run_has_complete_events(events: Arc<dyn EventStore>, key: &str) {
    let directory = tempfile::tempdir().unwrap();
    let store = sqlite_state(directory.path()).await;
    let app = app(store.clone(), events.clone());

    let run_id = submit(&app, key).await;
    let first = finished_stream(events.as_ref(), &run_id).await;
    wait_terminal(store.as_ref(), &run_id).await;
    assert!(first.len() >= 4, "first incarnation: {first:?}");

    assert_eq!(delete_status(&app, &run_id).await, StatusCode::OK);
    assert!(
        events
            .list_since(&run_id, None, 10)
            .await
            .unwrap()
            .is_empty()
    );

    assert_eq!(submit(&app, key).await, run_id);
    wait_terminal(store.as_ref(), &run_id).await;
    let second = finished_stream(events.as_ref(), &run_id).await;
    assert_eq!(shape(&second), shape(&first));
    assert!(
        second
            .iter()
            .all(|event| first.iter().all(|old| old.id != event.id)),
        "the second incarnation must publish its own events"
    );

    // Deleting the second incarnation fences it like the first.
    assert_eq!(delete_status(&app, &run_id).await, StatusCode::OK);
    assert_eq!(delete_status(&app, &run_id).await, StatusCode::NOT_FOUND);
    assert!(
        events
            .list_since(&run_id, None, 10)
            .await
            .unwrap()
            .is_empty()
    );
}

fn event(run_id: &str, id: &str) -> RunEvent {
    let mut event = RunEvent::run(
        run_id,
        "fence_contract",
        RunEventType::ContextUpdated,
        RunStatus::Running,
    );
    event.id = id.to_string();
    event
}

/// A `DELETE` that finds neither state nor events returns not-found and
/// leaves the ID usable; one that finds orphaned events removes and fences.
pub async fn assert_missing_run_delete_leaves_no_fence(events: &dyn EventStore, run_id: &str) {
    let directory = tempfile::tempdir().unwrap();
    let store = sqlite_state(directory.path()).await;

    let missing = lifecycle::delete_run(store.as_ref(), events, run_id)
        .await
        .unwrap_err();
    assert_eq!(missing.kind(), StorageErrorKind::NotFound);
    let kept = event(run_id, "after-missing-delete");
    events.publish(kept.clone()).await.unwrap();
    assert_eq!(
        events.list_since(run_id, None, 10).await.unwrap(),
        vec![kept]
    );

    // The event above is now an orphan: state never existed.
    lifecycle::delete_run(store.as_ref(), events, run_id)
        .await
        .unwrap();
    assert_eq!(
        events
            .publish(event(run_id, "after-orphan-delete"))
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );
    let again = lifecycle::delete_run(store.as_ref(), events, run_id)
        .await
        .unwrap_err();
    assert_eq!(again.kind(), StorageErrorKind::NotFound);
}

async fn assert_fenced(result: ironflow::storage::StorageResult<()>) {
    assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Conflict);
}

/// The fence follows incarnations: the deleted one stays rejected after the
/// run ID is admitted again, and so does a publisher that names none.
pub async fn assert_fence_is_scoped_to_one_incarnation(events: &dyn EventStore, run_id: &str) {
    // Before any admission every publisher is accepted. Admitting an
    // incarnation removes what earlier publishers left and makes it the
    // only one, deleted run or not (IF-214).
    assert_eq!(events.admitted_incarnation(run_id).await.unwrap(), None);
    events.publish(event(run_id, "unscoped-0")).await.unwrap();
    events.begin_incarnation(run_id, "first").await.unwrap();
    events.begin_incarnation(run_id, "first").await.unwrap();
    assert_eq!(
        events
            .admitted_incarnation(run_id)
            .await
            .unwrap()
            .as_deref(),
        Some("first")
    );
    assert!(
        events
            .list_since(run_id, None, 10)
            .await
            .unwrap()
            .is_empty()
    );
    events
        .publish_as(event(run_id, "first-1"), "first")
        .await
        .unwrap();
    assert_fenced(events.publish(event(run_id, "unscoped-1")).await).await;
    assert_fenced(
        events
            .publish_as(event(run_id, "zeroth-late"), "zeroth")
            .await,
    )
    .await;
    assert_eq!(events.delete_run(run_id).await.unwrap(), 1);
    assert_eq!(events.admitted_incarnation(run_id).await.unwrap(), None);

    assert_fenced(
        events
            .publish_as(event(run_id, "first-late"), "first")
            .await,
    )
    .await;
    assert_fenced(
        events
            .publish_as(event(run_id, "second-early"), "second")
            .await,
    )
    .await;
    assert_fenced(events.publish(event(run_id, "unscoped-late")).await).await;

    events.begin_incarnation(run_id, "second").await.unwrap();
    let admitted = event(run_id, "second-1");
    events.publish_as(admitted.clone(), "second").await.unwrap();
    events.publish_as(admitted.clone(), "second").await.unwrap();
    assert_fenced(
        events
            .publish_as(event(run_id, "first-later"), "first")
            .await,
    )
    .await;
    assert_fenced(events.publish(event(run_id, "unscoped-later")).await).await;
    assert_eq!(
        events.list_since(run_id, None, 10).await.unwrap(),
        vec![admitted]
    );

    // An orphan sweep that finds nothing leaves the admission in place.
    let other = format!("{run_id}-other");
    events.begin_incarnation(&other, "other").await.unwrap();
    assert_eq!(events.delete_orphaned_run(&other).await.unwrap(), 0);
    events
        .publish_as(event(&other, "other-1"), "other")
        .await
        .unwrap();

    // Deleting the second incarnation closes the stream to it as well.
    assert_eq!(events.delete_run(run_id).await.unwrap(), 1);
    assert_eq!(events.delete_run(run_id).await.unwrap(), 0);
    assert_fenced(
        events
            .publish_as(event(run_id, "second-late"), "second")
            .await,
    )
    .await;
    events.begin_incarnation(run_id, "third").await.unwrap();
    assert_eq!(events.delete_orphaned_run(run_id).await.unwrap(), 0);
    events
        .publish_as(event(run_id, "third-1"), "third")
        .await
        .unwrap();

    for invalid in ["", &"x".repeat(129)] {
        assert_eq!(
            events
                .begin_incarnation(run_id, invalid)
                .await
                .unwrap_err()
                .kind(),
            StorageErrorKind::InvalidInput
        );
        assert_eq!(
            events
                .publish_as(event(run_id, "invalid"), invalid)
                .await
                .unwrap_err()
                .kind(),
            StorageErrorKind::InvalidInput
        );
    }
}

/// Event store that pauses its first deletion until the test resumes it, so
/// a test can place a new submission between the state delete and the event
/// delete of one `DELETE /runs/{id}` (IF-214).
struct PausedDelete {
    inner: Arc<dyn EventStore>,
    armed: std::sync::atomic::AtomicBool,
    reached: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}

impl PausedDelete {
    fn new(inner: Arc<dyn EventStore>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            armed: std::sync::atomic::AtomicBool::new(false),
            reached: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        })
    }

    fn arm(&self) {
        self.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    async fn pause(&self) {
        if self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.reached.notify_one();
            self.resume.notified().await;
        }
    }
}

#[async_trait::async_trait]
impl EventStore for PausedDelete {
    async fn publish(&self, event: RunEvent) -> ironflow::storage::StorageResult<()> {
        self.inner.publish(event).await
    }

    async fn publish_as(
        &self,
        event: RunEvent,
        incarnation: &str,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.publish_as(event, incarnation).await
    }

    async fn begin_incarnation(
        &self,
        run_id: &str,
        incarnation: &str,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.begin_incarnation(run_id, incarnation).await
    }

    async fn admitted_incarnation(
        &self,
        run_id: &str,
    ) -> ironflow::storage::StorageResult<Option<String>> {
        self.inner.admitted_incarnation(run_id).await
    }

    async fn delete_run(&self, run_id: &str) -> ironflow::storage::StorageResult<usize> {
        self.pause().await;
        self.inner.delete_run(run_id).await
    }

    async fn delete_incarnation(
        &self,
        run_id: &str,
        incarnation: &str,
    ) -> ironflow::storage::StorageResult<usize> {
        self.pause().await;
        self.inner.delete_incarnation(run_id, incarnation).await
    }

    async fn delete_orphaned_run(&self, run_id: &str) -> ironflow::storage::StorageResult<usize> {
        self.pause().await;
        self.inner.delete_orphaned_run(run_id).await
    }

    async fn delete_orphaned_incarnation(
        &self,
        run_id: &str,
        seen: Option<&str>,
    ) -> ironflow::storage::StorageResult<usize> {
        self.pause().await;
        self.inner.delete_orphaned_incarnation(run_id, seen).await
    }

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> ironflow::storage::StorageResult<Vec<RunEvent>> {
        self.inner.list_since(run_id, after, limit).await
    }
}

/// Run `DELETE /runs/{id}` with a complete new submission of the same key
/// placed between its two steps, and return the `DELETE` status.
async fn delete_overlapped_by_resubmission(
    app: &Router,
    paused: &Arc<PausedDelete>,
    store: &dyn StateStore,
    key: &str,
    run_id: &str,
) -> StatusCode {
    paused.arm();
    let deleting = {
        let (app, run_id) = (app.clone(), run_id.to_string());
        tokio::spawn(async move { delete_status(&app, &run_id).await })
    };
    tokio::time::timeout(Duration::from_secs(20), paused.reached.notified())
        .await
        .expect("the delete never reached its event step");
    assert_eq!(submit(app, key).await, run_id);
    wait_terminal(store, run_id).await;
    paused.resume.notify_one();
    deleting.await.unwrap()
}

/// A `DELETE` whose event step runs after the same key was submitted again
/// must leave the new incarnation's stream complete and open (IF-214).
pub async fn assert_delete_overlapping_resubmission_keeps_new_events(
    events: Arc<dyn EventStore>,
    key: &str,
) {
    let directory = tempfile::tempdir().unwrap();
    let store = sqlite_state(directory.path()).await;
    let paused = PausedDelete::new(events.clone());
    let app = app(store.clone(), paused.clone());

    let run_id = submit(&app, key).await;
    let first = finished_stream(events.as_ref(), &run_id).await;
    wait_terminal(store.as_ref(), &run_id).await;

    let status =
        delete_overlapped_by_resubmission(&app, &paused, store.as_ref(), key, &run_id).await;
    assert_eq!(status, StatusCode::OK);
    let second = events.list_since(&run_id, None, 1_000).await.unwrap();
    assert_eq!(shape(&second), shape(&first), "the new run lost events");
    assert!(
        second
            .iter()
            .all(|event| first.iter().all(|old| old.id != event.id)),
        "events of the deleted incarnation survived"
    );

    // The new incarnation is still deletable and is fenced like any other.
    assert_eq!(delete_status(&app, &run_id).await, StatusCode::OK);
    assert!(
        events
            .list_since(&run_id, None, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(delete_status(&app, &run_id).await, StatusCode::NOT_FOUND);

    // A third incarnation still publishes a complete stream.
    assert_eq!(submit(&app, key).await, run_id);
    wait_terminal(store.as_ref(), &run_id).await;
    let third = finished_stream(events.as_ref(), &run_id).await;
    assert_eq!(shape(&third), shape(&first));
}

/// The same overlap on the path where state is already absent: a retried
/// `DELETE` sweeping orphaned events must not sweep a newer run's (IF-214).
pub async fn assert_orphan_sweep_overlapping_resubmission_keeps_new_events(
    events: Arc<dyn EventStore>,
    key: &str,
) {
    let directory = tempfile::tempdir().unwrap();
    let store = sqlite_state(directory.path()).await;
    let paused = PausedDelete::new(events.clone());
    let app = app(store.clone(), paused.clone());

    let run_id = submit(&app, key).await;
    let first = finished_stream(events.as_ref(), &run_id).await;
    wait_terminal(store.as_ref(), &run_id).await;
    // An interrupted earlier delete: state is gone, events are orphaned.
    store.delete_run(&run_id).await.unwrap();

    let status =
        delete_overlapped_by_resubmission(&app, &paused, store.as_ref(), key, &run_id).await;
    // The orphans were replaced by the new run before the sweep ran.
    assert_eq!(status, StatusCode::NOT_FOUND);
    let second = events.list_since(&run_id, None, 1_000).await.unwrap();
    assert_eq!(shape(&second), shape(&first), "the new run lost events");
    assert!(
        second
            .iter()
            .all(|event| first.iter().all(|old| old.id != event.id)),
        "orphaned events of the deleted incarnation survived"
    );
    assert_eq!(delete_status(&app, &run_id).await, StatusCode::OK);
}

/// Store-level form of the overlap: the event step of a delete names the
/// incarnation whose state was removed and is a no-op once the stream has
/// admitted a newer one (IF-214).
pub async fn assert_delete_is_scoped_to_the_removed_incarnation(
    events: &dyn EventStore,
    run_id: &str,
) {
    events.begin_incarnation(run_id, "old").await.unwrap();
    events
        .publish_as(event(run_id, "old-1"), "old")
        .await
        .unwrap();

    // State for "old" is deleted and the ID is accepted again before the
    // event step runs: admission removes the orphaned events.
    events.begin_incarnation(run_id, "new").await.unwrap();
    assert!(
        events
            .list_since(run_id, None, 10)
            .await
            .unwrap()
            .is_empty()
    );
    let first = event(run_id, "new-1");
    events.publish_as(first.clone(), "new").await.unwrap();

    assert_eq!(events.delete_incarnation(run_id, "old").await.unwrap(), 0);
    assert_eq!(
        events
            .delete_orphaned_incarnation(run_id, Some("old"))
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        events
            .delete_orphaned_incarnation(run_id, None)
            .await
            .unwrap(),
        0
    );
    let second = event(run_id, "new-2");
    events.publish_as(second.clone(), "new").await.unwrap();
    assert_eq!(
        events.list_since(run_id, None, 10).await.unwrap(),
        vec![first, second]
    );
    assert_fenced(events.publish_as(event(run_id, "old-late"), "old").await).await;

    // The sweep of a caller that saw this incarnation still removes it, and
    // the delete that names it removes and fences it.
    assert_eq!(
        events
            .delete_orphaned_incarnation(run_id, Some("new"))
            .await
            .unwrap(),
        2
    );
    events.begin_incarnation(run_id, "newer").await.unwrap();
    events
        .publish_as(event(run_id, "newer-1"), "newer")
        .await
        .unwrap();
    assert_eq!(events.delete_incarnation(run_id, "newer").await.unwrap(), 1);
    assert_eq!(events.delete_incarnation(run_id, "newer").await.unwrap(), 0);
    assert_fenced(
        events
            .publish_as(event(run_id, "newer-late"), "newer")
            .await,
    )
    .await;
    assert_eq!(
        events
            .delete_incarnation(run_id, "")
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::InvalidInput
    );
}
