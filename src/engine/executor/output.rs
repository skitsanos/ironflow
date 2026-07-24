use serde_json::Value;

use crate::engine::types::Context;
use crate::nodes::NodeFailure;

use super::overlay::ExecutionOverlay;

/// Output prepared for both context publication and bounded task history.
pub(super) struct PreparedOutput {
    context: Context,
    task_value: Value,
}

impl PreparedOutput {
    pub(super) fn task_value(&self) -> &Value {
        &self.task_value
    }

    pub(super) fn into_context(self) -> Context {
        self.context
    }
}

pub(super) fn prepare_output(output: &Context, overlay: &ExecutionOverlay) -> PreparedOutput {
    let context = overlay.redact_context(output);
    let task_value = bounded_task_value(&context);
    PreparedOutput {
        context,
        task_value,
    }
}

pub(super) fn prepare_failure_output(
    error: &anyhow::Error,
    overlay: &ExecutionOverlay,
) -> Option<PreparedOutput> {
    error
        .downcast_ref::<NodeFailure>()
        .map(|failure| prepare_output(failure.output(), overlay))
}

/// Cap individual oversized values in the final persisted context so a run
/// document cannot grow without bound (IF-048). This applies only to the durable
/// end-of-run snapshot used for inspection; it does not affect the in-flight
/// context that already carried full values between steps. Small values are
/// preserved; a value whose serialized form exceeds the per-task-output limit is
/// replaced with a truncation marker.
pub(super) fn bound_context(context: Context) -> Context {
    let limit = crate::util::limits::max_task_output_bytes() as usize;
    context
        .into_iter()
        .map(|(key, value)| {
            let size = value.to_string().len();
            if size > limit {
                (
                    key,
                    serde_json::json!({
                        "_truncated": true,
                        "_original_bytes": size,
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
    let value = Value::Object(
        output
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    );
    let serialized_size = value.to_string().len();
    let limit = crate::util::limits::max_task_output_bytes() as usize;

    if serialized_size > limit {
        serde_json::json!({
            "_truncated": true,
            "_original_bytes": serialized_size,
            "_limit_bytes": limit,
            "_note": "Output exceeded IRONFLOW_MAX_TASK_OUTPUT_BYTES and was omitted from task history.",
        })
    } else {
        value
    }
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
    fn structured_failure_output_is_redacted_before_publication() {
        let secret = "if022-secret-sentinel";
        let overlay = ExecutionOverlay::new(Context::from([(
            "_headers".to_string(),
            serde_json::json!({"authorization": secret}),
        )]));
        let failure = anyhow::Error::new(NodeFailure::new(
            "command failed",
            Context::from([
                ("shell_stderr".to_string(), serde_json::json!(secret)),
                ("shell_stdout".to_string(), serde_json::json!("safe")),
                ("_headers".to_string(), serde_json::json!({"copy": secret})),
            ]),
        ));

        let prepared = prepare_failure_output(&failure, &overlay).unwrap();
        let serialized = serde_json::to_string(&prepared.context).unwrap();

        assert!(!serialized.contains(secret));
        assert!(!prepared.context.contains_key("_headers"));
        assert_eq!(prepared.context["shell_stderr"], "[REDACTED]");
        assert_eq!(prepared.context["shell_stdout"], "safe");
    }
}
