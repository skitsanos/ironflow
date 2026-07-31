use base64::Engine as _;

use crate::engine::types::RunStatus;
use crate::util::sensitive_url::redact_sensitive_text;

use super::super::AppState;
use super::super::errors::AppError;

/// Generic message returned for any failed file-mode flow load.
pub(super) const FLOW_FILE_LOAD_ERROR: &str = "Failed to load flow file";
const FLOW_PATH_UNAVAILABLE: &str = "Flow file not found";

/// Log a failed file-mode flow load. The underlying Lua error echoes
/// file-derived tokens (`near '<token>'`) and confirms the named path is
/// readable, so it is logged (redacted) server-side and never returned to the
/// caller.
pub(super) fn log_flow_file_load_failure(path: &str, error: &anyhow::Error) {
    tracing::warn!(
        path = %path,
        error = %redact_sensitive_text(&format!("{error:#}")),
        "failed to load flow file"
    );
}

/// Public error for a failed file-mode flow load (generic; detail is logged).
pub(super) fn flow_file_load_error(path: &str, error: &anyhow::Error) -> AppError {
    log_flow_file_load_failure(path, error);
    AppError::BadRequest(FLOW_FILE_LOAD_ERROR.to_string())
}

pub(super) fn decode_base64_source(b64: &str) -> Result<String, AppError> {
    let max_bytes = crate::util::limits::max_flow_source_bytes();
    let decoded_bytes = canonical_base64_decoded_len(b64)?;
    if decoded_bytes > max_bytes {
        return Err(AppError::BadRequest(format!(
            "Decoded 'source_base64' exceeds the {max_bytes}-byte flow source limit (raise IRONFLOW_MAX_FLOW_SOURCE_BYTES to allow it)"
        )));
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| AppError::BadRequest(format!("Invalid base64 in 'source_base64': {}", e)))?;
    String::from_utf8(bytes)
        .map_err(|e| AppError::BadRequest(format!("Base64 payload is not valid UTF-8: {}", e)))
}

/// Return the exact output length for canonical padded base64 without
/// allocating its decoded buffer. The API uses the standard engine, whose
/// decoder requires a multiple-of-four input with at most two trailing `=`
/// bytes, so rejecting malformed structure here preserves the same contract.
fn canonical_base64_decoded_len(encoded: &str) -> Result<u64, AppError> {
    let bytes = encoded.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err(AppError::BadRequest(
            "Invalid base64 in 'source_base64': invalid encoded length".to_string(),
        ));
    }

    let padding = if bytes.ends_with(b"==") {
        2_u64
    } else if bytes.ends_with(b"=") {
        1_u64
    } else {
        0_u64
    };
    if bytes[..bytes.len().saturating_sub(padding as usize)].contains(&b'=') {
        return Err(AppError::BadRequest(
            "Invalid base64 in 'source_base64': invalid padding".to_string(),
        ));
    }

    u64::try_from(bytes.len() / 4)
        .ok()
        .and_then(|groups| groups.checked_mul(3))
        .and_then(|bytes| bytes.checked_sub(padding))
        .ok_or_else(|| {
            AppError::BadRequest(
                "Invalid base64 in 'source_base64': encoded length is too large".to_string(),
            )
        })
}

pub fn resolve_flow_path(file_path: &str, state: &AppState) -> Result<String, AppError> {
    resolve_flow_path_in(file_path, state.flows_dir.as_deref())
}

/// Resolve a configured or client-supplied flow path against an optional
/// sandbox root.
///
/// When `flows_dir` is configured, every accepted path — including absolute
/// paths — must canonicalize to a location inside that directory. The cwd
/// fallback is disabled in that mode to prevent execution of arbitrary `.lua`
/// files just because they are reachable from the server process. Lexically
/// outside paths are rejected before they are inspected, and every missing or
/// outside path returns the same public error.
///
/// When `flows_dir` is not configured there is no sandbox to enforce, and the
/// permissive behaviour (absolute or cwd-relative) is preserved.
pub fn resolve_flow_path_in(
    file_path: &str,
    flows_dir: Option<&std::path::Path>,
) -> Result<String, AppError> {
    if let Some(flows_dir) = flows_dir {
        let configured_root = absolute_configured_path(flows_dir).map_err(|e| {
            tracing::error!(
                error_kind = ?e.kind(),
                "configured flows_dir cannot be made absolute"
            );
            AppError::BadRequest("Configured flows_dir is not accessible".to_string())
        })?;
        let root = configured_root.canonicalize().map_err(|e| {
            tracing::error!(
                error_kind = ?e.kind(),
                "configured flows_dir is not accessible"
            );
            AppError::BadRequest("Configured flows_dir is not accessible".to_string())
        })?;

        let requested = std::path::Path::new(file_path);
        if requested
            .components()
            .any(|component| component == std::path::Component::ParentDir)
        {
            return Err(flow_path_unavailable("parent_component", None));
        }

        let candidate = if requested.is_absolute() {
            // Reject a lexical escape before calling exists/canonicalize on the
            // supplied path. Outside paths therefore cannot be used as an
            // existence oracle.
            if requested.starts_with(&root) {
                requested.to_path_buf()
            } else if let Ok(relative) = requested.strip_prefix(&configured_root) {
                // `flows_dir` itself may be a symlink. Map its configured
                // spelling onto the already-canonical root instead of probing
                // through that symlink again, which also closes a replacement
                // race between root validation and candidate inspection.
                root.join(relative)
            } else {
                return Err(flow_path_unavailable("absolute_outside_root", None));
            }
        } else {
            root.join(requested)
        };

        let canonical = candidate
            .canonicalize()
            .map_err(|error| flow_path_unavailable("canonicalize_failed", Some(error.kind())))?;

        if !canonical.starts_with(&root) {
            return Err(flow_path_unavailable("canonical_outside_root", None));
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

fn absolute_configured_path(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn flow_path_unavailable(reason: &'static str, error_kind: Option<std::io::ErrorKind>) -> AppError {
    tracing::debug!(
        reason,
        ?error_kind,
        "flow path is unavailable under configured flows_dir"
    );
    AppError::NotFound(FLOW_PATH_UNAVAILABLE.to_string())
}

pub(super) fn parse_status(s: &str) -> Result<RunStatus, String> {
    match s {
        "pending" => Ok(RunStatus::Pending),
        "running" => Ok(RunStatus::Running),
        "success" => Ok(RunStatus::Success),
        "failed" => Ok(RunStatus::Failed),
        "stalled" => Ok(RunStatus::Stalled),
        "cancelled" => Ok(RunStatus::Cancelled),
        _ => Err(format!(
            "Invalid status '{}'. Use: pending, running, success, failed, stalled, cancelled",
            s
        )),
    }
}
