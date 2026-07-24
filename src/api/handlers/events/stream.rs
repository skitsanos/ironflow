use std::collections::VecDeque;
use std::convert::Infallible;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use axum::response::sse::Event;
use serde::Serialize;
use uuid::Uuid;

use crate::api::errors::AppError;
use crate::engine::events::{RunEvent, RunEventType};
use crate::storage::event_store::EventStore;
use crate::storage::{StateStore, StorageError, StorageErrorKind};
use crate::util::sensitive_url::redact_sensitive_text;

use super::{BATCH_LIMIT, contains_forbidden_event_id_byte};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
type SseItem = Result<Event, Infallible>;

pub(super) struct EventStreamState {
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    run_id: String,
    after: Option<String>,
    pending: VecDeque<RunEvent>,
    wait_before_poll: bool,
    terminal_close_armed: bool,
    finished: bool,
}

impl EventStreamState {
    pub(super) fn new(
        store: Arc<dyn StateStore>,
        event_store: Arc<dyn EventStore>,
        run_id: String,
        after: Option<String>,
        initial_events: Vec<RunEvent>,
        terminal_close_armed: bool,
    ) -> Self {
        let wait_before_poll = initial_events.len() < BATCH_LIMIT;
        Self {
            store,
            event_store,
            run_id,
            after,
            pending: initial_events.into(),
            wait_before_poll,
            terminal_close_armed,
            finished: false,
        }
    }
}

#[derive(Serialize)]
struct StreamErrorPayload {
    error: &'static str,
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_id: Option<String>,
    retryable: bool,
}

pub(super) async fn next_stream_item(
    mut state: EventStreamState,
) -> Option<(SseItem, EventStreamState)> {
    if state.finished {
        return None;
    }

    loop {
        if let Some(run_event) = state.pending.pop_front() {
            if run_event.run_id != state.run_id {
                state.finished = true;
                let event = internal_stream_error(
                    &state.run_id,
                    "event_stream_error",
                    "Stored event belongs to a different run",
                    false,
                );
                return Some((Ok(event), state));
            }
            if !valid_stored_event_id(&run_event.id) {
                state.finished = true;
                let event = internal_stream_error(
                    &state.run_id,
                    "event_serialization_error",
                    "Stored event ID cannot be represented as SSE",
                    false,
                );
                return Some((Ok(event), state));
            }

            let event_id = run_event.id.clone();
            let terminal = run_event.event_type == RunEventType::RunFinished;
            let sse_event = match run_event_to_sse(run_event) {
                Ok(event) => event,
                Err(error) => {
                    state.finished = true;
                    let event = internal_stream_error(
                        &state.run_id,
                        "event_serialization_error",
                        error,
                        false,
                    );
                    return Some((Ok(event), state));
                }
            };

            state.after = Some(event_id);
            state.finished = terminal;
            return Some((Ok(sse_event), state));
        }

        if state.wait_before_poll {
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        match state
            .event_store
            .list_since(&state.run_id, state.after.as_deref(), BATCH_LIMIT)
            .await
        {
            Ok(events) if !events.is_empty() => {
                state.wait_before_poll = events.len() < BATCH_LIMIT;
                state.pending.extend(events);
            }
            Ok(_) if state.terminal_close_armed && state.wait_before_poll => {
                // Terminal status is persisted immediately before RunFinished is
                // published. This delayed empty poll is the final grace read.
                state.finished = true;
                return None;
            }
            Ok(_) if state.terminal_close_armed => {
                // A full page is followed immediately. Delay the final empty
                // read so a racing RunFinished publication remains observable.
                state.wait_before_poll = true;
            }
            Ok(_) => match state.store.get_run_info(&state.run_id).await {
                Ok(info) => {
                    state.wait_before_poll = true;
                    state.terminal_close_armed = info.status.is_terminal();
                }
                Err(error) => {
                    state.finished = true;
                    let retryable = storage_error_is_retryable(&error);
                    let event = internal_stream_error(
                        &state.run_id,
                        "event_stream_error",
                        error,
                        retryable,
                    );
                    return Some((Ok(event), state));
                }
            },
            Err(error) => {
                state.finished = true;
                let event = storage_stream_error(&state.run_id, error);
                return Some((Ok(event), state));
            }
        }
    }
}

fn run_event_to_sse(event: RunEvent) -> Result<Event, axum::Error> {
    Event::default()
        .id(&event.id)
        .event(event.event_type.as_sse_name())
        .json_data(event)
}

fn storage_stream_error(run_id: &str, error: StorageError) -> Event {
    if error.kind() == StorageErrorKind::NotFound {
        tracing::warn!(
            %run_id,
            error = %error,
            "Event stream cursor disappeared while polling"
        );
        return stream_error_event(StreamErrorPayload {
            error: "Event cursor is no longer available",
            code: "event_cursor_gone",
            error_id: None,
            retryable: false,
        });
    }

    let retryable = storage_error_is_retryable(&error);
    internal_stream_error(run_id, "event_stream_error", error, retryable)
}

fn internal_stream_error(
    run_id: &str,
    code: &'static str,
    diagnostic: impl fmt::Display,
    retryable: bool,
) -> Event {
    let error_id = Uuid::new_v4().to_string();
    let diagnostic = redact_sensitive_text(&diagnostic.to_string());
    tracing::error!(
        %error_id,
        %run_id,
        error = %diagnostic,
        "Event stream failed after the response started"
    );
    stream_error_event(StreamErrorPayload {
        error: if code == "event_serialization_error" {
            "Event serialization failed"
        } else {
            "Event stream unavailable"
        },
        code,
        error_id: Some(error_id),
        retryable,
    })
}

fn stream_error_event(payload: StreamErrorPayload) -> Event {
    let data = serde_json::to_string(&payload).unwrap_or_else(|_| {
        r#"{"error":"Event stream unavailable","code":"event_stream_error","retryable":true}"#
            .to_string()
    });
    Event::default().event("stream_error").data(data)
}

pub(super) fn validate_stored_events(
    events: &[RunEvent],
    expected_run_id: &str,
) -> Result<(), AppError> {
    for event in events {
        if event.run_id != expected_run_id || !valid_stored_event_id(&event.id) {
            return Err(AppError::Storage(StorageError::corruption(
                "Invalid stored event identity",
                "event does not match its run or cannot be represented as SSE",
            )));
        }
        if event.event_type == RunEventType::RunFinished {
            // RunFinished is the protocol EOF marker. Records after it are not
            // observable and therefore cannot invalidate an otherwise valid
            // terminal replay page.
            break;
        }
    }
    Ok(())
}

fn storage_error_is_retryable(error: &StorageError) -> bool {
    error.kind() == StorageErrorKind::Backend
}

fn valid_stored_event_id(id: &str) -> bool {
    !id.is_empty() && !contains_forbidden_event_id_byte(id)
}
