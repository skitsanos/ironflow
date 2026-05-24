use std::sync::Arc;

use axum::Json;
use axum::extract::State;

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
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
