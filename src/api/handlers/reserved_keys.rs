//! Context keys a caller may not define when starting a run.

use crate::engine::types::Context;

use super::super::errors::AppError;

/// Refuse a caller-supplied context that defines a key only the scheduler
/// writes. `subject` names the input in the error ("Request", "Webhook request").
pub(super) fn reject_scheduler_context_keys(
    context: &Context,
    subject: &str,
) -> Result<(), AppError> {
    reject_reserved_context_keys(
        context,
        subject,
        &crate::scheduler::SCHEDULER_OWNED_CONTEXT_KEYS,
    )
}

pub(super) fn reject_reserved_context_keys(
    context: &Context,
    subject: &str,
    reserved: &[&str],
) -> Result<(), AppError> {
    match reserved.iter().find(|key| context.contains_key(**key)) {
        Some(key) => Err(AppError::BadRequest(format!(
            "{subject} context must not define reserved key '{key}'"
        ))),
        None => Ok(()),
    }
}
