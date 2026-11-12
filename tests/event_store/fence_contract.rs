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

async fn submit(app: &Router, key: &str) -> String {
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
    assert!(response.status().is_success(), "{}", response.status());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    body["run_id"].as_str().unwrap().to_string()
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
    // Admitting an incarnation for a run that was never deleted records
    // nothing: every publisher is still accepted.
    events.begin_incarnation(run_id, "first").await.unwrap();
    events
        .publish_as(event(run_id, "first-1"), "first")
        .await
        .unwrap();
    events.publish(event(run_id, "unscoped-1")).await.unwrap();
    assert_eq!(events.delete_run(run_id).await.unwrap(), 2);

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
    events.publish(event(&other, "other-1")).await.unwrap();

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
