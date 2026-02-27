use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::engine::WorkflowEngine;
use crate::engine::types::{Context, RunStatus};
use crate::lua::LuaRuntime;
use crate::storage::StateStore;

use super::AppState;
use super::errors::AppError;

// --- Request/Response types ---

#[derive(Deserialize)]
pub struct RunFlowRequest {
    /// Inline Lua flow source code.
    #[serde(default)]
    pub source: Option<String>,
    /// Base64-encoded Lua flow source code (avoids JSON escaping issues).
    #[serde(default)]
    pub source_base64: Option<String>,
    /// Path to a .lua flow file (relative to flows_dir or absolute).
    #[serde(default)]
    pub file: Option<String>,
    /// Initial context for the workflow.
    #[serde(default)]
    pub context: Option<Context>,
}

#[derive(Serialize)]
pub struct RunFlowResponse {
    pub run_id: String,
    pub flow_name: String,
    pub status: String,
}

#[derive(Deserialize)]
pub struct ValidateFlowRequest {
    /// Inline Lua flow source code.
    #[serde(default)]
    pub source: Option<String>,
    /// Base64-encoded Lua flow source code (avoids JSON escaping issues).
    #[serde(default)]
    pub source_base64: Option<String>,
    /// Path to a .lua flow file.
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    pub flow_name: Option<String>,
    pub steps: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Deserialize)]
pub struct ListRunsQuery {
    pub status: Option<String>,
}

#[derive(Serialize)]
pub struct NodeInfo {
    pub node_type: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

// --- Handlers ---

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

    let engine = WorkflowEngine::new(
        state.registry.clone(),
        state.store.clone(),
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

    let runs = state.store.list_runs(status_filter).await?;

    // Return a summary view (without full context/task details)
    let summaries: Vec<serde_json::Value> = runs
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "flow_name": r.flow_name,
                "status": r.status,
                "started": r.started,
                "finished": r.finished,
                "task_count": r.tasks.len(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "runs": summaries,
        "total": summaries.len(),
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

/// POST /webhooks/{name}
pub async fn run_webhook(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    body: Option<Json<Context>>,
) -> Result<Json<RunFlowResponse>, AppError> {
    let flow_file = state
        .webhooks
        .get(&name)
        .ok_or_else(|| AppError::NotFound(format!("Webhook '{}' not found", name)))?;

    let path = resolve_flow_path(flow_file, &state)?;
    let flow = LuaRuntime::load_flow(&path, &state.registry)
        .map_err(|e| AppError::BadRequest(format!("Failed to load flow: {:#}", e)))?;

    let mut initial_ctx = body.map(|Json(ctx)| ctx).unwrap_or_default();

    // Inject request headers as _headers (lowercase keys)
    let headers_map: serde_json::Map<String, serde_json::Value> = headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str().ok().map(|val| {
                (
                    k.as_str().to_string(),
                    serde_json::Value::String(val.to_string()),
                )
            })
        })
        .collect();
    initial_ctx.insert(
        "_headers".to_string(),
        serde_json::Value::Object(headers_map),
    );

    // Inject webhook name
    initial_ctx.insert("_webhook".to_string(), serde_json::Value::String(name));
    let flow_name = flow.name.clone();

    // Inject _flow_dir for subworkflow path resolution
    if let Ok(resolved) = resolve_flow_path(flow_file, &state)
        && let Some(dir) = std::path::Path::new(&resolved).parent()
    {
        initial_ctx.insert(
            "_flow_dir".to_string(),
            serde_json::Value::String(dir.to_string_lossy().to_string()),
        );
    }

    let engine = WorkflowEngine::new(
        state.registry.clone(),
        state.store.clone(),
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

// --- Helpers ---

fn decode_base64_source(b64: &str) -> Result<String, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| AppError::BadRequest(format!("Invalid base64 in 'source_base64': {}", e)))?;
    String::from_utf8(bytes)
        .map_err(|e| AppError::BadRequest(format!("Base64 payload is not valid UTF-8: {}", e)))
}

fn resolve_flow_path(file_path: &str, state: &AppState) -> Result<String, AppError> {
    if std::path::Path::new(file_path).is_absolute() {
        return Ok(file_path.to_string());
    }

    if let Some(ref flows_dir) = state.flows_dir {
        let full_path = flows_dir.join(file_path);
        if full_path.exists() {
            return full_path
                .to_str()
                .map(|s| s.to_string())
                .ok_or_else(|| AppError::BadRequest("Invalid path".to_string()));
        }
    }

    // Try relative to cwd
    if std::path::Path::new(file_path).exists() {
        return Ok(file_path.to_string());
    }

    Err(AppError::NotFound(format!(
        "Flow file not found: {}",
        file_path
    )))
}

fn parse_status(s: &str) -> Result<RunStatus, String> {
    match s {
        "pending" => Ok(RunStatus::Pending),
        "running" => Ok(RunStatus::Running),
        "success" => Ok(RunStatus::Success),
        "failed" => Ok(RunStatus::Failed),
        "stalled" => Ok(RunStatus::Stalled),
        _ => Err(format!(
            "Invalid status '{}'. Use: pending, running, success, failed, stalled",
            s
        )),
    }
}
