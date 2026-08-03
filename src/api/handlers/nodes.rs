use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::super::AppState;
use super::types::{HealthResponse, NodeInfo};

/// GET /nodes
pub async fn list_nodes(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let nodes: Vec<NodeInfo> = state
        .registry
        .list()
        .iter()
        .map(|(name, desc)| NodeInfo {
            node_type: name.to_string(),
            description: desc.to_string(),
        })
        .collect();

    let total = nodes.len();
    Json(serde_json::json!({
        "nodes": nodes,
        "total": total,
    }))
}

/// GET /health
pub async fn health() -> Json<HealthResponse> {
    Json(health_response("ok"))
}

/// GET /health/live — process liveness only.
pub async fn liveness() -> Json<HealthResponse> {
    Json(health_response("ok"))
}

/// GET /health/ready — admission state plus bounded durable-store probes.
pub async fn readiness(State(state): State<Arc<AppState>>) -> Response {
    if !state.lifecycle.is_ready() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(health_response("draining")),
        )
            .into_response();
    }

    let probes = async {
        tokio::try_join!(state.store.healthcheck(), state.event_store.healthcheck())?;
        Ok::<(), crate::storage::StorageError>(())
    };
    match tokio::time::timeout(std::time::Duration::from_secs(2), probes).await {
        Ok(Ok(())) if state.lifecycle.is_ready() => {
            (StatusCode::OK, Json(health_response("ready"))).into_response()
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, "readiness storage probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(health_response("not_ready")),
            )
                .into_response()
        }
        Ok(Ok(())) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(health_response("draining")),
        )
            .into_response(),
        Err(_) => {
            tracing::warn!("readiness storage probe timed out");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(health_response("not_ready")),
            )
                .into_response()
        }
    }
}

fn health_response(status: &str) -> HealthResponse {
    HealthResponse {
        status: status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
