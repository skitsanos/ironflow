use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::engine::WorkflowEngine;
use crate::lua::LuaRuntime;

use super::super::AppState;
use super::super::errors::AppError;
use super::helpers::{decode_base64_source, resolve_flow_path};
use super::types::{RunFlowRequest, RunFlowResponse, ValidateFlowRequest, ValidateResponse};

/// POST /flows/run
pub async fn run_flow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunFlowRequest>,
) -> Result<Json<RunFlowResponse>, AppError> {
    let source_count = [
        req.source.is_some(),
        req.source_base64.is_some(),
        req.file.is_some(),
    ]
    .iter()
    .filter(|&&v| v)
    .count();

    if source_count == 0 {
        return Err(AppError::BadRequest(
            "Exactly one of 'source', 'source_base64', or 'file' is required".to_string(),
        ));
    }
    if source_count > 1 {
        return Err(AppError::BadRequest(
            "Only one of 'source', 'source_base64', or 'file' may be provided".to_string(),
        ));
    }

    let flow = if let Some(source) = &req.source {
        LuaRuntime::load_flow_from_string(source, &state.registry)
            .map_err(|e| AppError::BadRequest(format!("Failed to parse flow: {:#}", e)))?
    } else if let Some(b64) = &req.source_base64 {
        let source = decode_base64_source(b64)?;
        LuaRuntime::load_flow_from_string(&source, &state.registry)
            .map_err(|e| AppError::BadRequest(format!("Failed to parse flow: {:#}", e)))?
    } else {
        let file_path = req.file.as_ref().unwrap();
        let path = resolve_flow_path(file_path, &state)?;
        LuaRuntime::load_flow(&path, &state.registry)
            .map_err(|e| AppError::BadRequest(format!("Failed to load flow: {:#}", e)))?
    };

    let mut initial_ctx = req.context.unwrap_or_default();
    let flow_name = flow.name.clone();

    // Inject _flow_dir for subworkflow path resolution
    if let Some(ref file_path) = req.file {
        if let Ok(resolved) = resolve_flow_path(file_path, &state)
            && let Some(dir) = std::path::Path::new(&resolved).parent()
        {
            initial_ctx.insert(
                "_flow_dir".to_string(),
                serde_json::Value::String(dir.to_string_lossy().to_string()),
            );
        }
    } else if let Some(ref flows_dir) = state.flows_dir {
        initial_ctx.insert(
            "_flow_dir".to_string(),
            serde_json::Value::String(flows_dir.to_string_lossy().to_string()),
        );
    }

    let engine = WorkflowEngine::new_with_events(
        state.registry.clone(),
        state.store.clone(),
        state.event_store.clone(),
        state.max_concurrent_tasks,
    );
    let run_id = engine.execute(&flow, initial_ctx).await?;

    let run_info = state.store.get_run_info(&run_id).await?;

    Ok(Json(RunFlowResponse {
        run_id,
        flow_name,
        status: run_info.status.to_string(),
    }))
}

/// POST /flows/validate
pub async fn validate_flow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ValidateFlowRequest>,
) -> Result<Json<ValidateResponse>, AppError> {
    let source_count = [
        req.source.is_some(),
        req.source_base64.is_some(),
        req.file.is_some(),
    ]
    .iter()
    .filter(|&&v| v)
    .count();

    if source_count == 0 {
        return Err(AppError::BadRequest(
            "Exactly one of 'source', 'source_base64', or 'file' is required".to_string(),
        ));
    }
    if source_count > 1 {
        return Err(AppError::BadRequest(
            "Only one of 'source', 'source_base64', or 'file' may be provided".to_string(),
        ));
    }

    let flow_result = if let Some(source) = &req.source {
        LuaRuntime::load_flow_from_string(source, &state.registry)
    } else if let Some(b64) = &req.source_base64 {
        let source = decode_base64_source(b64)?;
        LuaRuntime::load_flow_from_string(&source, &state.registry)
    } else {
        let file_path = req.file.as_ref().unwrap();
        let path = resolve_flow_path(file_path, &state)?;
        LuaRuntime::load_flow(&path, &state.registry)
    };

    match flow_result {
        Ok(flow) => {
            let mut errors = Vec::new();

            // Check node types exist
            for step in &flow.steps {
                if state.registry.get(&step.node_type).is_none() {
                    errors.push(format!(
                        "Step '{}' uses unknown node type '{}'",
                        step.name, step.node_type
                    ));
                }
            }

            // Validate DAG (dependencies + cycle detection)
            errors.extend(flow.validate_dag());

            Ok(Json(ValidateResponse {
                valid: errors.is_empty(),
                flow_name: Some(flow.name),
                steps: Some(flow.steps.len()),
                errors,
            }))
        }
        Err(e) => Ok(Json(ValidateResponse {
            valid: false,
            flow_name: None,
            steps: None,
            errors: vec![format!("{:#}", e)],
        })),
    }
}
