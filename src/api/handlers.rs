use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use base64::Engine as _;
use futures_util::Stream;
use serde::{Deserialize, Serialize};

use crate::engine::WorkflowEngine;
use crate::engine::types::{Context, RunStatus};
use crate::lua::LuaRuntime;

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
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Deserialize)]
pub struct RunEventsQuery {
    pub after: Option<String>,
}

/// Default page size when `?limit` is not supplied.
pub const DEFAULT_LIST_RUNS_LIMIT: usize = 50;
/// Hard cap on `?limit` to prevent one request from loading the whole catalog.
pub const MAX_LIST_RUNS_LIMIT: usize = 500;

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

/// GET /runs/:id/events
pub async fn run_events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<RunEventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, AppError> {
    state
        .store
        .get_run_info(&id)
        .await
        .map_err(|_| AppError::NotFound(format!("Run '{}' not found", id)))?;

    const BATCH_LIMIT: usize = 100;
    let stream_state = (state.event_store.clone(), id, params.after);
    let stream =
        futures_util::stream::unfold(stream_state, |(event_store, run_id, after)| async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tick.tick().await;
                let events = event_store
                    .list_since(&run_id, after.as_deref(), BATCH_LIMIT)
                    .await
                    .unwrap_or_default();

                if let Some(event) = events.into_iter().next() {
                    let next_after = Some(event.id.clone());
                    let sse_event = Event::default()
                        .id(event.id.clone())
                        .event(event.event_type.as_sse_name())
                        .json_data(event)
                        .unwrap_or_else(|_| Event::default().event("event_serialization_error"));
                    return Some((Ok(sse_event), (event_store, run_id, next_after)));
                }
            }
        });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
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

// --- Helpers ---

fn decode_base64_source(b64: &str) -> Result<String, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| AppError::BadRequest(format!("Invalid base64 in 'source_base64': {}", e)))?;
    String::from_utf8(bytes)
        .map_err(|e| AppError::BadRequest(format!("Base64 payload is not valid UTF-8: {}", e)))
}

/// Resolve a client-supplied flow path.
///
/// When `flows_dir` is configured, every accepted path — including absolute
/// paths — must canonicalize to a location inside that directory. The cwd
/// fallback is disabled in that mode to prevent a caller from executing
/// arbitrary `.lua` files just because they are reachable from the server
/// process.
///
/// When `flows_dir` is not configured there is no sandbox to enforce, and the
/// old permissive behaviour (absolute or cwd-relative) is preserved.
pub fn resolve_flow_path(file_path: &str, state: &AppState) -> Result<String, AppError> {
    if let Some(ref flows_dir) = state.flows_dir {
        let root = flows_dir.canonicalize().map_err(|e| {
            AppError::BadRequest(format!(
                "Configured flows_dir '{}' is not accessible: {}",
                flows_dir.display(),
                e
            ))
        })?;

        let candidate = if std::path::Path::new(file_path).is_absolute() {
            std::path::PathBuf::from(file_path)
        } else {
            root.join(file_path)
        };

        if !candidate.exists() {
            return Err(AppError::NotFound(format!(
                "Flow file not found: {}",
                file_path
            )));
        }

        let canonical = candidate.canonicalize().map_err(|e| {
            AppError::BadRequest(format!("Cannot resolve flow path '{}': {}", file_path, e))
        })?;

        if !canonical.starts_with(&root) {
            return Err(AppError::Forbidden(format!(
                "Flow path '{}' escapes configured flows_dir",
                file_path
            )));
        }

        return canonical
            .to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| AppError::BadRequest("Invalid path encoding".to_string()));
    }

    if std::path::Path::new(file_path).is_absolute() {
        return Ok(file_path.to_string());
    }
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
