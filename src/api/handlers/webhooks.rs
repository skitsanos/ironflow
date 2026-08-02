use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};

use crate::engine::WorkflowEngine;
use crate::engine::types::Context;
use crate::lua::LuaRuntime;

use super::super::AppState;
use super::super::errors::AppError;
use super::helpers::{flow_file_load_error, resolve_flow_path};
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

    // Reserve run capacity before evaluating the flow. The parser has its own
    // small process-wide ceiling because each Lua VM has a substantial memory
    // budget even when the eventual run is refused.
    let run_permit = crate::api::acquire_run_permit()?;
    let flow_load_permit = crate::api::acquire_flow_load_permit()?;
    // Parse off the async runtime so a pathological flow cannot pin a worker
    // thread and stall the whole server (IF-038).
    let registry = state.registry.clone();
    let load_path = path.clone();
    let flow = crate::api::supervise_flow_load(flow_load_permit, async move {
        LuaRuntime::load_flow_async(&load_path, &registry).await
    })
    .await
    .map_err(|e| flow_file_load_error(&path, &e))?;

    // Inject webhook name
    initial_ctx.insert("_webhook".to_string(), serde_json::Value::String(name));
    let flow_name = flow.name.clone();

    // Inject _flow_dir for subworkflow path resolution
    if let Some(dir) = std::path::Path::new(&path).parent() {
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
    let handle = engine
        .start_with_overlay(&flow, initial_ctx, execution_overlay)
        .await?;
    let run_id = handle.id().to_string();
    crate::api::wait_for_admitted_run(handle, run_permit).await?;

    let run_info = state.store.get_run_info(&run_id).await?;

    Ok(Json(RunFlowResponse {
        run_id,
        flow_name,
        status: run_info.status.to_string(),
    }))
}
