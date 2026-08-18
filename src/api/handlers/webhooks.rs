use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};

use crate::engine::WorkflowEngine;
use crate::engine::types::Context;
use crate::lua::LuaRuntime;

use super::super::AppState;
use super::super::errors::AppError;
use super::super::webhook_signature::SignatureVerificationError;
use super::helpers::{flow_file_load_error, resolve_flow_path};
use super::types::RunFlowResponse;

/// POST /webhooks/{name}
pub async fn run_webhook(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Json<RunFlowResponse>, AppError> {
    let webhook = state
        .webhooks
        .get(&name)
        .ok_or_else(|| AppError::NotFound(format!("Webhook '{}' not found", name)))?;

    webhook
        .verify_signature(&headers, &body)
        .map_err(|error| match error {
            SignatureVerificationError::Rejected => {
                AppError::Forbidden("invalid webhook signature".to_string())
            }
            SignatureVerificationError::Misconfigured(message) => {
                AppError::Internal(anyhow::anyhow!(message))
            }
        })?;

    let path = resolve_flow_path(webhook.flow(), &state)?;
    let mut initial_ctx = if body.is_empty() {
        Context::new()
    } else {
        if !has_json_content_type(&headers) {
            return Err(AppError::UnsupportedMediaType(
                "webhook request body must use an application/json content type".to_string(),
            ));
        }
        Json::<Context>::from_bytes(&body)
            .map(|Json(context)| context)
            .map_err(|_| {
                AppError::BadRequest("webhook request body must be a JSON object".to_string())
            })?
    };
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
    let run_permit = crate::api::acquire_run_permit(&state.lifecycle, state.metrics.as_deref())?;
    let flow_load_permit = crate::api::acquire_flow_load_permit(state.metrics.as_deref())?;
    // Parse off the async runtime so a pathological flow cannot pin a worker
    // thread and stall the whole server (IF-038).
    let registry = state.registry.clone();
    let load_path = path.clone();
    let flow = crate::api::supervise_flow_load(
        flow_load_permit,
        async move { LuaRuntime::load_flow_async(&load_path, &registry).await },
        state.metrics.clone(),
    )
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
    )
    .with_metrics(state.metrics.clone());
    let handle = engine
        .start_with_overlay(&flow, initial_ctx, execution_overlay)
        .await?;
    let run_id = handle.id().to_string();
    crate::api::wait_for_admitted_run(state.lifecycle.clone(), handle, run_permit).await?;

    let run_info = state.store.get_run_info(&run_id).await?;

    Ok(Json(RunFlowResponse {
        run_id,
        flow_name,
        status: run_info.status.to_string(),
    }))
}

fn has_json_content_type(headers: &axum::http::HeaderMap) -> bool {
    let Some(content_type) = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    media_type == "application/json"
        || media_type
            .strip_prefix("application/")
            .is_some_and(|subtype| subtype.ends_with("+json"))
}

#[cfg(test)]
mod tests {
    use super::has_json_content_type;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn recognizes_json_media_types() {
        for value in [
            "application/json",
            "application/json; charset=utf-8",
            "application/vnd.example+json",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("content-type", HeaderValue::from_str(value).unwrap());
            assert!(has_json_content_type(&headers), "{value}");
        }

        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("text/plain"));
        assert!(!has_json_content_type(&headers));
    }
}
