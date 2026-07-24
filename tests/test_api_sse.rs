use std::sync::Arc;

use axum::http::{HeaderValue, StatusCode};
use ironflow::api::errors::ERROR_ID_HEADER;
use ironflow::engine::RunEventType;
use ironflow::engine::types::RunStatus;
use ironflow::storage::event_store::{EventStore as _, MemoryEventStore};
use ironflow::storage::{StateStore as _, StorageError};

#[path = "api_sse/support.rs"]
mod support;

use support::{
    RUN_ID, ScriptedEventStore, event_ids, frame_json, frames, harness, response_json,
    response_text, run_event,
};

#[tokio::test]
async fn replay_drains_every_page_in_order_and_stops_at_run_finished() {
    let events = Arc::new(MemoryEventStore::new());
    let mut expected_ids = Vec::new();
    for index in 0..205 {
        let id = format!("event-{index:03}");
        expected_ids.push(id.clone());
        let event_type = if index == 204 {
            RunEventType::RunFinished
        } else if index == 0 {
            RunEventType::RunStarted
        } else {
            RunEventType::ContextUpdated
        };
        events.publish(run_event(id, event_type)).await.unwrap();
    }
    events
        .publish(run_event(
            "must-not-emit-after-terminal\nunsafe",
            RunEventType::ContextUpdated,
        ))
        .await
        .unwrap();

    let app = harness(events, RunStatus::Success).await;
    let response = app.request(&format!("/runs/{RUN_ID}/events"), &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;

    assert_eq!(event_ids(&body), expected_ids);
    assert_eq!(body.matches("event: run_finished").count(), 1);
    assert!(!body.contains("must-not-emit-after-terminal"));
}

#[tokio::test]
async fn terminal_event_hides_unobservable_invalid_records_in_the_initial_page() {
    let events = Arc::new(MemoryEventStore::new());
    events
        .publish(run_event("terminal", RunEventType::RunFinished))
        .await
        .unwrap();
    events
        .publish(run_event("unsafe\nid", RunEventType::ContextUpdated))
        .await
        .unwrap();

    let app = harness(events, RunStatus::Success).await;
    let response = app.request(&format!("/runs/{RUN_ID}/events"), &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert_eq!(event_ids(&body), vec!["terminal"]);
    assert!(!body.contains("stream_error"));
}

#[tokio::test]
async fn last_event_id_overrides_query_and_empty_header_falls_back() {
    let events = Arc::new(MemoryEventStore::new());
    for event in [
        run_event("bootstrap", RunEventType::RunStarted),
        run_event("événement-2", RunEventType::ContextUpdated),
        run_event("terminal", RunEventType::RunFinished),
    ] {
        events.publish(event).await.unwrap();
    }
    let app = harness(events, RunStatus::Success).await;
    let unicode_cursor = HeaderValue::from_bytes("événement-2".as_bytes()).unwrap();

    let response = app
        .request(
            &format!("/runs/{RUN_ID}/events?after=bootstrap"),
            &[unicode_cursor],
        )
        .await;
    assert_eq!(event_ids(&response_text(response).await), vec!["terminal"]);

    let response = app
        .request(
            &format!("/runs/{RUN_ID}/events?after=bootstrap"),
            &[HeaderValue::from_static("")],
        )
        .await;
    assert_eq!(
        event_ids(&response_text(response).await),
        vec!["événement-2", "terminal"]
    );

    let response = app
        .request(&format!("/runs/{RUN_ID}/events?after=terminal"), &[])
        .await;
    assert!(response_text(response).await.is_empty());
}

#[tokio::test]
async fn unavailable_cursor_is_410_without_internal_error_id() {
    let app = harness(Arc::new(MemoryEventStore::new()), RunStatus::Running).await;
    let response = app
        .request(&format!("/runs/{RUN_ID}/events?after=expired"), &[])
        .await;

    assert_eq!(response.status(), StatusCode::GONE);
    assert!(!response.headers().contains_key(ERROR_ID_HEADER));
    let body = response_json(response).await;
    assert_eq!(body["code"], "event_cursor_gone");
    assert!(body.get("error_id").is_none());
}

#[tokio::test]
async fn invalid_or_duplicate_last_event_id_is_bad_request() {
    let app = harness(Arc::new(MemoryEventStore::new()), RunStatus::Running).await;
    let response = app
        .request(&format!("/runs/{RUN_ID}/events?after=%00bad"), &[])
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .request(
            &format!("/runs/{RUN_ID}/events"),
            &[
                HeaderValue::from_static("one"),
                HeaderValue::from_static("two"),
            ],
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn initial_event_backend_failure_uses_generic_correlated_http_500() {
    const SENTINEL: &str = "initial-backend-secret";
    let events = Arc::new(ScriptedEventStore::new(vec![Err(StorageError::backend(
        "read events",
        format!("postgres://operator:{SENTINEL}@db.test/events"),
    ))]));
    let app = harness(events, RunStatus::Running).await;
    let response = app.request(&format!("/runs/{RUN_ID}/events"), &[]).await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let error_id = response.headers()[ERROR_ID_HEADER].to_str().unwrap();
    uuid::Uuid::parse_str(error_id).unwrap();
    let body = response_json(response).await;
    assert_eq!(body["code"], "internal_error");
    assert!(!body.to_string().contains(SENTINEL));
}

#[tokio::test]
async fn post_200_backend_failure_emits_one_idless_retryable_error_then_eof() {
    let events = Arc::new(ScriptedEventStore::new(vec![
        Ok(vec![run_event("delivered", RunEventType::RunStarted)]),
        Err(StorageError::backend(
            "poll events",
            "redis://operator:poll-secret@cache.test/0",
        )),
    ]));
    let app = harness(events.clone(), RunStatus::Running).await;
    let response = app.request(&format!("/runs/{RUN_ID}/events"), &[]).await;
    let body = response_text(response).await;

    assert_eq!(body.matches("event: stream_error").count(), 1);
    let error_frame = frames(&body)
        .into_iter()
        .find(|frame| frame.contains("event: stream_error"))
        .unwrap();
    assert!(!error_frame.lines().any(|line| line.starts_with("id: ")));
    let payload = frame_json(error_frame);
    assert_eq!(payload["code"], "event_stream_error");
    assert_eq!(payload["retryable"], true);
    uuid::Uuid::parse_str(payload["error_id"].as_str().unwrap()).unwrap();
    assert!(!body.contains("poll-secret"));
    assert_eq!(events.cursors().await, vec![None, Some("delivered".into())]);
}

#[tokio::test]
async fn post_200_cursor_loss_is_nonretryable_uncorrelated_and_closes() {
    let events = Arc::new(ScriptedEventStore::new(vec![
        Ok(vec![run_event("delivered", RunEventType::RunStarted)]),
        Err(StorageError::not_found("cursor expired")),
    ]));
    let app = harness(events, RunStatus::Running).await;
    let response = app.request(&format!("/runs/{RUN_ID}/events"), &[]).await;
    let body = response_text(response).await;
    let error_frame = frames(&body)
        .into_iter()
        .find(|frame| frame.contains("event: stream_error"))
        .unwrap();
    let payload = frame_json(error_frame);

    assert_eq!(payload["code"], "event_cursor_gone");
    assert_eq!(payload["retryable"], false);
    assert!(payload.get("error_id").is_none());
    assert!(!error_frame.lines().any(|line| line.starts_with("id: ")));
}

#[tokio::test]
async fn post_200_corruption_is_nonretryable_correlated_and_closes() {
    let events = Arc::new(ScriptedEventStore::new(vec![
        Ok(vec![run_event("delivered", RunEventType::RunStarted)]),
        Err(StorageError::corruption(
            "decode events",
            "stored event JSON is invalid",
        )),
    ]));
    let app = harness(events, RunStatus::Running).await;
    let body = response_text(app.request(&format!("/runs/{RUN_ID}/events"), &[]).await).await;
    let error_frame = frames(&body)
        .into_iter()
        .find(|frame| frame.contains("event: stream_error"))
        .unwrap();
    let payload = frame_json(error_frame);

    assert_eq!(payload["code"], "event_stream_error");
    assert_eq!(payload["retryable"], false);
    uuid::Uuid::parse_str(payload["error_id"].as_str().unwrap()).unwrap();
    assert!(!error_frame.lines().any(|line| line.starts_with("id: ")));
}

#[tokio::test]
async fn terminal_state_fallback_gracefully_closes_without_run_finished() {
    let events = Arc::new(MemoryEventStore::new());
    events
        .publish(run_event("only-event", RunEventType::RunStarted))
        .await
        .unwrap();
    let app = harness(events, RunStatus::Running).await;
    let response = app.request(&format!("/runs/{RUN_ID}/events"), &[]).await;

    app.store
        .set_run_status(RUN_ID, RunStatus::Stalled)
        .await
        .unwrap();
    let body = response_text(response).await;
    assert_eq!(event_ids(&body), vec!["only-event"]);
    assert!(!body.contains("event: stream_error"));
}

#[tokio::test]
async fn unsafe_event_id_after_200_becomes_correlated_nonretryable_stream_error() {
    let events = Arc::new(ScriptedEventStore::new(vec![
        Ok(vec![run_event("delivered", RunEventType::RunStarted)]),
        Ok(vec![run_event("unsafe\nid", RunEventType::ContextUpdated)]),
    ]));
    let app = harness(events, RunStatus::Running).await;
    let body = response_text(app.request(&format!("/runs/{RUN_ID}/events"), &[]).await).await;
    let error_frame = frames(&body)
        .into_iter()
        .find(|frame| frame.contains("event: stream_error"))
        .unwrap();
    let payload = frame_json(error_frame);

    assert_eq!(payload["code"], "event_serialization_error");
    assert_eq!(payload["retryable"], false);
    uuid::Uuid::parse_str(payload["error_id"].as_str().unwrap()).unwrap();
    assert_eq!(event_ids(&body), vec!["delivered"]);
}
