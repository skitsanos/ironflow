use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
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
    let flow = match (&req.source, &req.file) {
        (Some(source), _) => {
            LuaRuntime::load_flow_from_string(source, &state.registry)
                .map_err(|e| AppError::BadRequest(format!("Failed to parse flow: {:#}", e)))?
        }
        (_, Some(file_path)) => {
            let path = resolve_flow_path(file_path, &state)?;
            LuaRuntime::load_flow(&path, &state.registry)
                .map_err(|e| AppError::BadRequest(format!("Failed to load flow: {:#}", e)))?
        }
        (None, None) => {
            return Err(AppError::BadRequest(
                "Either 'source' (inline Lua) or 'file' (path) is required".to_string(),
            ));
        }
    };

    let initial_ctx = req.context.unwrap_or_default();
    let flow_name = flow.name.clone();

    let engine = WorkflowEngine::new(state.registry.clone(), state.store.clone());
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
    let flow_result = match (&req.source, &req.file) {
        (Some(source), _) => LuaRuntime::load_flow_from_string(source, &state.registry),
        (_, Some(file_path)) => {
            let path = resolve_flow_path(file_path, &state)?;
            LuaRuntime::load_flow(&path, &state.registry)
        }
        (None, None) => {
            return Err(AppError::BadRequest(
                "Either 'source' or 'file' is required".to_string(),
            ));
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
pub async fn list_nodes(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
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

// --- Helpers ---

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
