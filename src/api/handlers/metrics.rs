use std::sync::Arc;

use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::IntoResponse;

use super::super::AppState;
use super::super::errors::AppError;

const OPENMETRICS_CONTENT_TYPE: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";

/// GET /metrics -- registered only when the operator enables metrics.
pub async fn metrics(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, AppError> {
    let metrics = state.metrics.as_ref().ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "metrics route was registered without a metrics registry"
        ))
    })?;
    let body = metrics
        .encode()
        .map_err(|_| AppError::Internal(anyhow::anyhow!("failed to encode operator metrics")))?;

    Ok((
        [
            (CONTENT_TYPE, OPENMETRICS_CONTENT_TYPE),
            (CACHE_CONTROL, "no-store"),
        ],
        body,
    ))
}
