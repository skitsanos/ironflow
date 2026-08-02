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

pub(super) fn prepare_output(output: Context, overlay: &ExecutionOverlay) -> PreparedOutput {
    let context = overlay.redact_context_owned(output);
    let task_value = bounded_task_value(&context);
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
        .map(|(key, value)| {
            if serialized_size_up_to(&value, limit) == SizeBound::Exceeded {
                (
                    key,
                    serde_json::json!({
                        "_truncated": true,
                        "_minimum_bytes": limit.saturating_add(1),
                        "_limit_bytes": limit,
                        "_note": "Value exceeded IRONFLOW_MAX_TASK_OUTPUT_BYTES and was truncated in the persisted final context.",
                    }),
                )
            } else {
                (key, value)
            }
        })
        .collect()
}

fn bounded_task_value(output: &Context) -> Value {
    let limit = task_output_limit();

    if serialized_size_up_to(output, limit) == SizeBound::Exceeded {
        serde_json::json!({
            "_truncated": true,
            "_minimum_bytes": limit.saturating_add(1),
            "_limit_bytes": limit,
            "_note": "Output exceeded IRONFLOW_MAX_TASK_OUTPUT_BYTES and was omitted from task history.",
        })
    } else {
        Value::Object(
            output
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        )
    }
}

fn task_output_limit() -> usize {
    usize::try_from(crate::util::limits::max_task_output_bytes()).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_context_truncates_only_oversized_values() {
        // IF-048: the persisted final context caps individual oversized values
        // (default limit 2 MiB) while preserving small ones.
        let mut context = Context::new();
        context.insert("small".to_string(), serde_json::json!("hello"));
        context.insert(
            "big".to_string(),
            serde_json::Value::String("x".repeat(3 * 1024 * 1024)),
        );

        let bounded = bound_context(context);
        assert_eq!(bounded.get("small").unwrap(), &serde_json::json!("hello"));
        assert_eq!(
            bounded.get("big").unwrap().get("_truncated").unwrap(),
            &serde_json::json!(true)
        );
    }

    #[test]
    fn oversized_task_output_keeps_the_original_context_allocation() {
        let text = "x".repeat(3 * 1024 * 1024);
        let pointer = text.as_ptr();
        let prepared = prepare_output(
            Context::from([("big".to_string(), Value::String(text))]),
            &ExecutionOverlay::default(),
        );

        assert_eq!(prepared.context["big"].as_str().unwrap().as_ptr(), pointer);
        assert_eq!(prepared.task_value["_truncated"], true);
    }

    #[test]
    fn small_task_output_is_preserved_in_context_and_history() {
        let output = Context::from([
            ("count".to_string(), serde_json::json!(3)),
            ("message".to_string(), serde_json::json!("complete")),
        ]);

        let prepared = prepare_output(output.clone(), &ExecutionOverlay::default());

        assert_eq!(prepared.context, output);
        assert_eq!(
            prepared.task_value,
            serde_json::json!({"count": 3, "message": "complete"})
        );
    }

    #[test]
    fn uniquely_owned_final_context_reuses_value_allocation() {
        let text = "x".repeat(4096);
        let pointer = text.as_ptr();
        let context = Arc::new(Context::from([("result".to_string(), Value::String(text))]));

        let context = into_owned_context(context);

        assert_eq!(context["result"].as_str().unwrap().as_ptr(), pointer);
    }

    #[test]
    fn structured_failure_output_is_taken_without_cloning() {
        let text = "x".repeat(4096);
        let pointer = text.as_ptr();
        let mut failure = anyhow::Error::new(NodeFailure::new(
            "failed",
            Context::from([("diagnostic".to_string(), Value::String(text))]),
        ));

        let prepared = prepare_failure_output(&mut failure, &ExecutionOverlay::default()).unwrap();

        assert_eq!(
            prepared.context["diagnostic"].as_str().unwrap().as_ptr(),
            pointer
        );
        assert!(
            failure
                .downcast_ref::<NodeFailure>()
                .unwrap()
                .output()
                .is_empty()
        );
    }

    #[test]
    fn structured_failure_output_is_redacted_before_publication() {
        let secret = "if022-secret-sentinel";
        let overlay = ExecutionOverlay::new(Context::from([(
            "_headers".to_string(),
            serde_json::json!({"authorization": secret}),
        )]));
        let mut failure = anyhow::Error::new(NodeFailure::new(
            "command failed",
            Context::from([
                ("shell_stderr".to_string(), serde_json::json!(secret)),
                ("shell_stdout".to_string(), serde_json::json!("safe")),
                ("_headers".to_string(), serde_json::json!({"copy": secret})),
            ]),
        ));

        let prepared = prepare_failure_output(&mut failure, &overlay).unwrap();
        let serialized = serde_json::to_string(&prepared.context).unwrap();

        assert!(!serialized.contains(secret));
        assert!(!prepared.context.contains_key("_headers"));
        assert_eq!(prepared.context["shell_stderr"], "[REDACTED]");
        assert_eq!(prepared.context["shell_stdout"], "safe");
    }
}
