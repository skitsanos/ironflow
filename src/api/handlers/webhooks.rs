use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};

use crate::engine::WorkflowEngine;
use crate::engine::types::Context;
use crate::lua::LuaRuntime;

use super::super::AppState;
use super::super::errors::AppError;
use super::helpers::resolve_flow_path;
use super::types::RunFlowResponse;

/// POST /webhooks/{name}
pub async fn run_webhook(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    body: Option<Json<Context>>,
) -> Result<Json<RunFlowResponse>, AppError> {
    let webhook = state
        .webhooks
        .get(&name)
        .ok_or_else(|| AppError::NotFound(format!("Webhook '{}' not found", name)))?;

    let path = resolve_flow_path(webhook.flow(), &state)?;
    let flow = LuaRuntime::load_flow(&path, &state.registry)
        .map_err(|e| AppError::BadRequest(format!("Failed to load flow: {:#}", e)))?;

    let mut initial_ctx = body.map(|Json(ctx)| ctx).unwrap_or_default();
    for reserved in ["_headers", "_webhook", "_flow_dir"] {
        if initial_ctx.contains_key(reserved) {
            return Err(AppError::BadRequest(format!(
                "Webhook request context must not define reserved key '{reserved}'"
            )));
        }
    }
    let execution_overlay = webhook
        .execution_overlay(&headers)
        .map_err(AppError::BadRequest)?;

    // Inject webhook name
    initial_ctx.insert("_webhook".to_string(), serde_json::Value::String(name));
    let flow_name = flow.name.clone();

    // Inject _flow_dir for subworkflow path resolution
    if let Ok(resolved) = resolve_flow_path(webhook.flow(), &state)
        && let Some(dir) = std::path::Path::new(&resolved).parent()
    {
        initial_ctx.insert(
            "_flow_dir".to_string(),
            serde_json::Value::String(dir.to_string_lossy().to_string()),
        );
    }

    let engine = WorkflowEngine::new_with_events(
        state.registry.clone(),
        state.store.clone(),
        state.event_store.clone(),
        state.max_concurrent_tasks,
    );
    let run_id = engine
        .execute_with_overlay(&flow, initial_ctx, execution_overlay)
        .await?;

    let run_info = state.store.get_run_info(&run_id).await?;

    Ok(Json(RunFlowResponse {
        run_id,
        flow_name,
        status: run_info.status.to_string(),
    }))
}
