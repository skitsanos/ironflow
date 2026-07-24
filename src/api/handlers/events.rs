mod stream;

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;

use crate::engine::events::RunEvent;
use crate::storage::event_store::EventStore;
use crate::storage::{StorageError, StorageErrorKind, validate_run_id};

use self::stream::{EventStreamState, next_stream_item, validate_stored_events};
use super::super::AppState;
use super::super::errors::AppError;
use super::types::RunEventsQuery;

pub(super) const BATCH_LIMIT: usize = 100;
const LAST_EVENT_ID_HEADER: &str = "last-event-id";

/// GET /runs/:id/events
pub async fn run_events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<RunEventsQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    validate_run_id(&id).map_err(StorageError::invalid_input)?;
    let run_info = state.store.get_run_info(&id).await?;
    let after = effective_cursor(&headers, params.after)?;
    let initial_events = initial_batch(state.event_store.as_ref(), &id, after.as_deref()).await?;
    validate_stored_events(&initial_events, &id)?;

    let stream_state = EventStreamState::new(
        state.store.clone(),
        state.event_store.clone(),
        id,
        after,
        initial_events,
        run_info.status.is_terminal(),
    );
    let stream = futures_util::stream::unfold(stream_state, next_stream_item);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

fn effective_cursor(
    headers: &HeaderMap,
    query_after: Option<String>,
) -> Result<Option<String>, AppError> {
    let mut header_values = headers.get_all(LAST_EVENT_ID_HEADER).iter();
    let header_cursor = header_values
        .next()
        .map(|value| {
            std::str::from_utf8(value.as_bytes())
                .map(str::to_owned)
                .map_err(|_| AppError::BadRequest("Last-Event-ID must be valid UTF-8".to_string()))
        })
        .transpose()?;
    if header_values.next().is_some() {
        return Err(AppError::BadRequest(
            "Last-Event-ID must appear at most once".to_string(),
        ));
    }

    // Last-Event-ID is the reconnect cursor. It must override a fixed
    // bootstrap `?after` value when EventSource reconnects to the same URL.
    match normalize_cursor(header_cursor, "Last-Event-ID")? {
        Some(cursor) => Ok(Some(cursor)),
        None => normalize_cursor(query_after, "after"),
    }
}

fn normalize_cursor(cursor: Option<String>, source: &str) -> Result<Option<String>, AppError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    if cursor.is_empty() {
        return Ok(None);
    }
    if contains_forbidden_event_id_byte(&cursor) {
        return Err(AppError::BadRequest(format!(
            "{source} must not contain NUL, CR, or LF"
        )));
    }
    Ok(Some(cursor))
}

async fn initial_batch(
    event_store: &dyn EventStore,
    run_id: &str,
    after: Option<&str>,
) -> Result<Vec<RunEvent>, AppError> {
    match event_store.list_since(run_id, after, BATCH_LIMIT).await {
        Ok(events) => Ok(events),
        Err(error) if after.is_some() && error.kind() == StorageErrorKind::NotFound => Err(
            AppError::EventCursorGone("Event cursor is no longer available".to_string()),
        ),
        Err(error) => Err(AppError::Storage(error)),
    }
}

pub(super) fn contains_forbidden_event_id_byte(id: &str) -> bool {
    id.as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'\0' | b'\r' | b'\n'))
}
