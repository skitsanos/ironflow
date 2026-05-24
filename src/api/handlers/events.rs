use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;

use super::super::AppState;
use super::super::errors::AppError;
use super::types::RunEventsQuery;

/// GET /runs/:id/events
pub async fn run_events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<RunEventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, AppError> {
    state
        .store
        .get_run_info(&id)
        .await
        .map_err(|_| AppError::NotFound(format!("Run '{}' not found", id)))?;

    const BATCH_LIMIT: usize = 100;
    let stream_state = (state.event_store.clone(), id, params.after);
    let stream =
        futures_util::stream::unfold(stream_state, |(event_store, run_id, after)| async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tick.tick().await;
                let events = event_store
                    .list_since(&run_id, after.as_deref(), BATCH_LIMIT)
                    .await
                    .unwrap_or_default();

                if let Some(event) = events.into_iter().next() {
                    let next_after = Some(event.id.clone());
                    let sse_event = Event::default()
                        .id(event.id.clone())
                        .event(event.event_type.as_sse_name())
                        .json_data(event)
                        .unwrap_or_else(|_| Event::default().event("event_serialization_error"));
                    return Some((Ok(sse_event), (event_store, run_id, next_after)));
                }
            }
        });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
