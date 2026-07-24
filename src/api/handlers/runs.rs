use std::sync::Arc;

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};

use crate::storage::{RunCursor, RunListQuery, StorageError, validate_run_id};

use super::super::AppState;
use super::super::errors::AppError;
use super::helpers::parse_status;
use super::types::ListRunsQuery;

/// GET /runs
pub async fn list_runs(
    State(state): State<Arc<AppState>>,
    params: Result<Query<ListRunsQuery>, QueryRejection>,
) -> Result<Json<serde_json::Value>, AppError> {
    let Query(params) = params.map_err(|error| AppError::BadRequest(error.to_string()))?;
    let status_filter = params
        .status
        .as_deref()
        .map(parse_status)
        .transpose()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    if params.offset.is_some() {
        return Err(AppError::BadRequest(
            "offset pagination is not supported; use the `after` cursor".to_string(),
        ));
    }
    let limit = state
        .listing_policy
        .api_page_size(params.limit)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    let after = params.after.as_deref().map(RunCursor::decode).transpose()?;
    let query = RunListQuery::new(status_filter, after, limit)?;
    let page = state.store.list_run_summaries_page(&query).await?;
    let returned = page.items.len();
    let has_more = page.has_more();
    let next_cursor = page.next.map(|cursor| cursor.encode()).transpose()?;

    let summaries: Vec<serde_json::Value> = page
        .items
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "flow_name": r.flow_name,
                "status": r.status,
                "started": r.started,
                "finished": r.finished,
                "task_count": r.task_count,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "runs": summaries,
        "limit": limit.get(),
        "returned": returned,
        "has_more": has_more,
        "next_cursor": next_cursor,
    })))
}

/// GET /runs/:id
pub async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_run_id(&id).map_err(StorageError::invalid_input)?;
    let info = state.store.get_run_info(&id).await?;

    let mut value = serde_json::to_value(&info).map_err(anyhow::Error::from)?;
    crate::util::redaction::redact_legacy_webhook_record(&mut value);
    Ok(Json(value))
}

/// DELETE /runs/:id
pub async fn delete_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_run_id(&id).map_err(StorageError::invalid_input)?;
    crate::storage::lifecycle::delete_run(state.store.as_ref(), state.event_store.as_ref(), &id)
        .await?;

    Ok(Json(serde_json::json!({
        "deleted": id,
    })))
}
