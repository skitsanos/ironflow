use base64::Engine as _;

use crate::engine::types::RunStatus;

use super::super::AppState;
use super::super::errors::AppError;

pub(super) fn decode_base64_source(b64: &str) -> Result<String, AppError> {
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

pub(super) fn parse_status(s: &str) -> Result<RunStatus, String> {
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
