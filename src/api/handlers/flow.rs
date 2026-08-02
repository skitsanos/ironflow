use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::engine::WorkflowEngine;
use crate::lua::LuaRuntime;

use super::super::AppState;
use super::super::errors::AppError;
use super::helpers::{
    FLOW_FILE_LOAD_ERROR, decode_base64_source, flow_file_load_error, log_flow_file_load_failure,
    resolve_flow_path,
};
use super::types::{RunFlowRequest, RunFlowResponse, ValidateFlowRequest, ValidateResponse};

/// POST /flows/run
pub async fn run_flow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunFlowRequest>,
) -> Result<Json<RunFlowResponse>, AppError> {
    let RunFlowRequest {
        source,
        source_base64,
        file,
        context,
    } = req;
    let source_count = [source.is_some(), source_base64.is_some(), file.is_some()]
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

    reject_inline_source_when_disabled(&state, source.is_some() || source_base64.is_some())?;

    // Resolve the confined file path before reserving scarce execution/parser
    // capacity. The resolved value is reused for loading and `_flow_dir`.
    let resolved_file = match file.as_deref() {
        Some(file) => Some(resolve_flow_path(file, &state)?),
        None => None,
    };

    // Run admission covers the whole expensive lifecycle, including Lua flow
    // evaluation. Acquiring after parsing let rejected requests each allocate
    // a separate bounded-but-large VM before receiving 503.
    let run_permit = crate::api::acquire_run_permit()?;
    let flow_load_permit = crate::api::acquire_flow_load_permit()?;

    // Parse off the async runtime so a pathological flow cannot pin a worker
    // thread and stall the whole server (IF-038).
    let flow = if let Some(source) = source {
        let registry = state.registry.clone();
        crate::api::supervise_flow_load(flow_load_permit, async move {
            LuaRuntime::load_flow_from_string_async(&source, &registry).await
        })
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to parse flow: {:#}", e)))?
    } else if let Some(b64) = source_base64 {
        let source = decode_base64_source(&b64)?;
        let registry = state.registry.clone();
        crate::api::supervise_flow_load(flow_load_permit, async move {
            LuaRuntime::load_flow_from_string_async(&source, &registry).await
        })
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to parse flow: {:#}", e)))?
    } else {
        let path = resolved_file
            .as_deref()
            .expect("source validation guarantees one resolved file")
            .to_string();
        let registry = state.registry.clone();
        let load_path = path.clone();
        crate::api::supervise_flow_load(flow_load_permit, async move {
            LuaRuntime::load_flow_async(&load_path, &registry).await
        })
        .await
        .map_err(|e| flow_file_load_error(&path, &e))?
    };

    let mut initial_ctx = context.unwrap_or_default();
    let flow_name = flow.name.clone();

    // Inject _flow_dir for subworkflow path resolution
    if let Some(resolved) = resolved_file {
        if let Some(dir) = std::path::Path::new(&resolved).parent() {
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
    let handle = engine.start(&flow, initial_ctx).await?;
    let run_id = handle.id().to_string();
    crate::api::wait_for_admitted_run(handle, run_permit).await?;

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
    let ValidateFlowRequest {
        source,
        source_base64,
        file,
    } = req;
    let source_count = [source.is_some(), source_base64.is_some(), file.is_some()]
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

    // Validation executes the top-level Lua chunk, including `env()`, so it
    // has the same trust boundary as execution. Reject before decoding or
    // evaluating any caller-controlled source (IF-061).
    reject_inline_source_when_disabled(&state, source.is_some() || source_base64.is_some())?;

    let resolved_file = match file.as_deref() {
        Some(file) => Some(resolve_flow_path(file, &state)?),
        None => None,
    };
    let flow_load_permit = crate::api::acquire_flow_load_permit()?;

    // Parse off the async runtime (IF-038).
    let flow_result = if let Some(source) = source {
        let registry = state.registry.clone();
        crate::api::supervise_flow_load(flow_load_permit, async move {
            LuaRuntime::load_flow_from_string_async(&source, &registry).await
        })
        .await
    } else if let Some(b64) = source_base64 {
        let source = decode_base64_source(&b64)?;
        let registry = state.registry.clone();
        crate::api::supervise_flow_load(flow_load_permit, async move {
            LuaRuntime::load_flow_from_string_async(&source, &registry).await
        })
        .await
    } else {
        let path = resolved_file
            .as_deref()
            .expect("source validation guarantees one resolved file")
            .to_string();
        let registry = state.registry.clone();
        let load_path = path.clone();
        // File-mode parse errors must not echo file-derived detail (IF-045);
        // return a generic validation failure instead of the raw Lua error.
        match crate::api::supervise_flow_load(flow_load_permit, async move {
            LuaRuntime::load_flow_async(&load_path, &registry).await
        })
        .await
        {
            Ok(flow) => Ok(flow),
            Err(e) => {
                log_flow_file_load_failure(&path, &e);
                return Ok(Json(ValidateResponse {
                    valid: false,
                    flow_name: None,
                    steps: None,
                    errors: vec![FLOW_FILE_LOAD_ERROR.to_string()],
                }));
            }
        }
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

/// Inline source evaluates arbitrary caller-controlled Lua. A deployment that
/// exposes only its configured flow catalog must apply this boundary to both
/// execution and validation; file mode remains confined by `flows_dir`.
fn reject_inline_source_when_disabled(
    state: &AppState,
    has_inline_source: bool,
) -> Result<(), AppError> {
    if !state.allow_adhoc_flows && has_inline_source {
        return Err(AppError::Forbidden(
            "Inline flow source is disabled on this server (allow_adhoc_flows: false). \
             Use 'file' to access a flow already present in the configured flows directory."
                .to_string(),
        ));
    }

    // Disabling ad-hoc evaluation only creates a fixed catalog when file
    // requests have an explicit root. Without this defense, an API caller
    // could still name any absolute or cwd-existing Lua file readable by the
    // process. Server startup rejects this configuration too; keep the handler
    // check for directly-constructed routers and future embedding callers.
    if !state.allow_adhoc_flows && state.flows_dir.is_none() {
        return Err(AppError::Forbidden(
            "File-based flow access requires a configured flows_dir when ad-hoc flows are disabled"
                .to_string(),
        ));
    }

    Ok(())
}
