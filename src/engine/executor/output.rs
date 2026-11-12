use std::borrow::Cow;
use std::sync::Arc;

use serde_json::Value;

use crate::engine::types::Context;
use crate::nodes::NodeFailure;

use super::overlay::ExecutionOverlay;

mod size;

use size::{SizeBound, serialized_size_up_to};

/// Output prepared for both context publication and bounded task history.
pub(super) struct PreparedOutput {
    context: Context,
    task_value: Value,
}

impl PreparedOutput {
    pub(super) fn into_parts(self) -> (Context, Value) {
        (self.context, self.task_value)
    }
}

pub(super) fn split_failure_output(
    output: Option<PreparedOutput>,
) -> (Option<Arc<Context>>, Option<Value>) {
    let Some(output) = output else {
        return (None, None);
    };
    let (context, task_value) = output.into_parts();
    (Some(Arc::new(context)), Some(task_value))
}

pub(super) fn into_owned_context(context: Arc<Context>) -> Context {
    Arc::try_unwrap(context).unwrap_or_else(|shared| shared.as_ref().clone())
}

pub(super) fn prepare_final_context(
    context: Arc<Context>,
    overlay: &ExecutionOverlay,
    retain_live: bool,
) -> (Context, Option<Context>) {
    let context = into_owned_context(context);
    if !retain_live {
        return (bound_context(overlay.redact_context_owned(context)), None);
    }
    // A parent run reads the live values, which keep their JSON types; the
    // stored snapshot gets the record form. Clone only values admitted to
    // inspection, never the oversized live values.
    let live = overlay.redact_live_context(context);
    let limit = task_output_limit();
    let snapshot = live
        .iter()
        .filter(|(key, _)| !overlay.withholds_record_key(key))
        .map(|(key, value)| (key.clone(), record_value(value, overlay, limit)))
        .collect();
    (snapshot, Some(live))
}

/// Publish output to the running workflow and derive its task-history record.
///
/// The live half keeps JSON types and engine routing keys, so an overlay
/// value cannot reroute steps or change the shape of data between them
/// (IF-176). The history half is the stricter record form.
pub(super) fn prepare_output(output: Context, overlay: &ExecutionOverlay) -> PreparedOutput {
    let context = overlay.redact_live_context(output);
    let task_value = bounded_task_value(&context, overlay);
    PreparedOutput {
        context,
        task_value,
    }
}

pub(super) fn prepare_failure_output(
    error: &mut anyhow::Error,
    overlay: &ExecutionOverlay,
) -> Option<PreparedOutput> {
    error
        .downcast_mut::<NodeFailure>()
        .map(|failure| prepare_output(failure.take_output(), overlay))
}

/// Cap individual oversized values in the final persisted context so a run
/// document cannot grow without bound (IF-048). This applies only to the durable
/// end-of-run snapshot used for inspection; it does not affect the in-flight
/// context that already carried full values between steps. Small values are
/// preserved; a value whose serialized form exceeds the per-task-output limit is
/// replaced with a truncation marker.
pub(super) fn bound_context(context: Context) -> Context {
    let limit = task_output_limit();
    context
        .into_iter()
        .map(|(key, value)| (key, context_truncation(&value, limit).unwrap_or(value)))
        .collect()
}

fn context_truncation(value: &Value, limit: usize) -> Option<Value> {
    (serialized_size_up_to(value, limit) == SizeBound::Exceeded).then(|| serde_json::json!({
        "_truncated": true,
        "_minimum_bytes": limit.saturating_add(1),
        "_limit_bytes": limit,
        "_note": "Value exceeded IRONFLOW_MAX_TASK_OUTPUT_BYTES and was truncated in the persisted final context.",
    }))
}

/// Record form of one live value: scrubbed for storage, then capped.
fn record_value(value: &Value, overlay: &ExecutionOverlay, limit: usize) -> Value {
    if let Some(marker) = context_truncation(value, limit) {
        return marker;
    }
    if overlay.is_empty() {
        return value.clone();
    }
    let record = overlay.redact_record_value(value);
    // Replacing a number with the redaction marker can lengthen the value.
    context_truncation(&record, limit).unwrap_or(record)
}

fn bounded_task_value(output: &Context, overlay: &ExecutionOverlay) -> Value {
    let limit = task_output_limit();
    if serialized_size_up_to(output, limit) == SizeBound::Exceeded {
        return omitted_task_output(limit);
    }
    match overlay.redact_context(output) {
        Cow::Borrowed(output) => Value::Object(
            output
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ),
        // Replacing a number with the redaction marker can lengthen the record.
        Cow::Owned(record) if serialized_size_up_to(&record, limit) == SizeBound::Exceeded => {
            omitted_task_output(limit)
        }
        Cow::Owned(record) => Value::Object(record.into_iter().collect()),
    }
}

fn omitted_task_output(limit: usize) -> Value {
    serde_json::json!({
        "_truncated": true,
        "_minimum_bytes": limit.saturating_add(1),
        "_limit_bytes": limit,
        "_note": "Output exceeded IRONFLOW_MAX_TASK_OUTPUT_BYTES and was omitted from task history.",
    })
}

fn task_output_limit() -> usize {
    usize::try_from(crate::util::limits::max_task_output_bytes()).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
