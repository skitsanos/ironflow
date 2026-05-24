use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};

use super::super::AppState;
use super::super::errors::AppError;
use super::helpers::parse_status;
use super::types::{DEFAULT_LIST_RUNS_LIMIT, ListRunsQuery, MAX_LIST_RUNS_LIMIT};

/// GET /runs
pub async fn list_runs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListRunsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let status_filter = params
        .status
        .as_deref()
        .map(parse_status)
        .transpose()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let limit = params
        .limit
        .unwrap_or(DEFAULT_LIST_RUNS_LIMIT)
        .clamp(1, MAX_LIST_RUNS_LIMIT);
    let offset = params.offset.unwrap_or(0);

    // Storage returns lightweight summaries — no ctx, no task payloads.
    // Default impl still loads full runs under the hood; concrete stores
    // (JSON, Redis) can override `list_run_summaries` for a real win.
    let mut summaries_all = state.store.list_run_summaries(status_filter).await?;
    summaries_all.sort_by_key(|summary| std::cmp::Reverse(summary.started));

    let total_matching = summaries_all.len();
    let page: Vec<&crate::engine::types::RunSummary> =
        summaries_all.iter().skip(offset).take(limit).collect();

    let summaries: Vec<serde_json::Value> = page
        .iter()
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
        "total": total_matching,
        "limit": limit,
        "offset": offset,
        "returned": summaries.len(),
    })))
}

/// GET /runs/:id
pub async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let info = state
        .store
        .get_run_info(&id)
        .await
        .map_err(|_| AppError::NotFound(format!("Run '{}' not found", id)))?;

    Ok(Json(serde_json::to_value(&info).unwrap()))
}

/// DELETE /runs/:id
pub async fn delete_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Check it exists first
    state
        .store
        .get_run_info(&id)
        .await
        .map_err(|_| AppError::NotFound(format!("Run '{}' not found", id)))?;

    state.store.delete_run(&id).await?;

    Ok(Json(serde_json::json!({
        "deleted": id,
    })))
}
