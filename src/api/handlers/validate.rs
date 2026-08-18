use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::api::AppState;
use crate::api::errors::AppError;
use crate::lua::LuaRuntime;

use super::flow::reject_inline_source_when_disabled;
use super::helpers::{
    FLOW_FILE_LOAD_ERROR, decode_base64_source, log_flow_file_load_failure, resolve_flow_path,
};
use super::types::{ValidateFlowRequest, ValidateResponse};

/// POST /flows/validate
pub async fn validate_flow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ValidateFlowRequest>,
) -> Result<Json<ValidateResponse>, AppError> {
    let ValidateFlowRequest {
        source,
        source_base64,
        file,
        strict,
    } = req;
    let source_count = [source.is_some(), source_base64.is_some(), file.is_some()]
        .iter()
        .filter(|&&v| v)
        .count();
    if source_count != 1 {
        let message = if source_count == 0 {
            "Exactly one of 'source', 'source_base64', or 'file' is required"
        } else {
            "Only one of 'source', 'source_base64', or 'file' may be provided"
        };
        return Err(AppError::BadRequest(message.to_string()));
    }

    // Validation executes the top-level Lua chunk, including `env()`, so it
    // has the same trust boundary as execution. Reject before decoding or
    // evaluating any caller-controlled source (IF-061).
    reject_inline_source_when_disabled(&state, source.is_some() || source_base64.is_some())?;
    let resolved_file = file
        .as_deref()
        .map(|file| resolve_flow_path(file, &state))
        .transpose()?;
    let flow_load_permit = crate::api::acquire_flow_load_permit(state.metrics.as_deref())?;

    let flow_result = if let Some(source) = source {
        let registry = state.registry.clone();
        crate::api::supervise_flow_load(
            flow_load_permit,
            async move { LuaRuntime::validate_flow_from_string_async(&source, &registry).await },
            state.metrics.clone(),
        )
        .await
    } else if let Some(encoded) = source_base64 {
        let source = decode_base64_source(&encoded)?;
        let registry = state.registry.clone();
        crate::api::supervise_flow_load(
            flow_load_permit,
            async move { LuaRuntime::validate_flow_from_string_async(&source, &registry).await },
            state.metrics.clone(),
        )
        .await
    } else {
        let path = resolved_file.expect("source validation guarantees one resolved file");
        let registry = state.registry.clone();
        let load_path = path.clone();
        match crate::api::supervise_flow_load(
            flow_load_permit,
            async move { LuaRuntime::validate_flow_async(&load_path, &registry).await },
            state.metrics.clone(),
        )
        .await
        {
            Ok(flow) => Ok(flow),
            Err(error) => {
                log_flow_file_load_failure(&path, &error);
                return Ok(Json(ValidateResponse {
                    valid: false,
                    flow_name: None,
                    steps: None,
                    errors: vec![FLOW_FILE_LOAD_ERROR.to_string()],
                    warnings: Vec::new(),
                }));
            }
        }
    };

    Ok(Json(match flow_result {
        Ok(validated) => {
            let flow = validated.flow;
            let mut errors = flow
                .steps
                .iter()
                .filter(|step| state.registry.get(&step.node_type).is_none())
                .map(|step| {
                    format!(
                        "Step '{}' uses unknown node type '{}'",
                        step.name, step.node_type
                    )
                })
                .collect::<Vec<_>>();
            errors.extend(flow.validate_dag());
            if strict && !validated.warnings.is_empty() {
                errors.push(format!(
                    "Strict validation rejected {} Lua handler warning(s)",
                    validated.warnings.len()
                ));
            }
            ValidateResponse {
                valid: errors.is_empty(),
                flow_name: Some(flow.name),
                steps: Some(flow.steps.len()),
                errors,
                warnings: validated.warnings,
            }
        }
        Err(error) => ValidateResponse {
            valid: false,
            flow_name: None,
            steps: None,
            errors: vec![format!("{error:#}")],
            warnings: Vec::new(),
        },
    }))
}
