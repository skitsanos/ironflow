use std::sync::Arc;

use jiff::Timestamp;

use crate::engine::types::Context;

/// Snapshot the shared workflow context for one node invocation.
///
/// Invocation-local values override shared values, but they are applied only
/// to the returned snapshot. They reach the shared workflow context only when
/// a node explicitly includes them in its output.
pub(super) fn task_input_context(
    phase_ctx: &Arc<Context>,
    execution_overlay: &Context,
    input_overlay: Option<&Context>,
) -> Arc<Context> {
    if execution_overlay.is_empty() && input_overlay.is_none() {
        return phase_ctx.clone();
    }

    let mut invocation = phase_ctx.as_ref().clone();
    invocation.extend(
        execution_overlay
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    let Some(input_overlay) = input_overlay else {
        return Arc::new(invocation);
    };
    invocation.extend(
        input_overlay
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    Arc::new(invocation)
}

/// Compute duration in milliseconds between two optional timestamps.
pub(super) fn task_duration_ms(
    started: Option<Timestamp>,
    finished: Option<Timestamp>,
) -> Option<u64> {
    crate::util::timestamp::elapsed_millis(started?, finished?)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Recorded before the engine's timestamps moved to `jiff` (IF-231).
    #[test]
    fn task_duration_is_whole_milliseconds_and_never_negative() {
        let at = |seconds, nanos| Some(Timestamp::new(seconds, nanos).unwrap());

        assert_eq!(task_duration_ms(at(10, 0), at(10, 0)), Some(0));
        assert_eq!(
            task_duration_ms(at(10, 0), at(11, 500_999_999)),
            Some(1_500)
        );
        assert_eq!(
            task_duration_ms(at(10, 999_999), at(10, 1_000_000)),
            Some(0)
        );
        assert_eq!(
            task_duration_ms(at(-1, 999_000_000), at(0, 1_000_000)),
            Some(2)
        );
        assert_eq!(task_duration_ms(at(10, 1_000_000), at(10, 0)), None);
        assert_eq!(task_duration_ms(None, at(10, 0)), None);
        assert_eq!(task_duration_ms(at(10, 0), None), None);
    }

    #[test]
    fn input_overlays_do_not_mutate_the_phase_snapshot() {
        let shared = Arc::new(Context::from([
            ("shared".to_string(), json!(true)),
            ("_error_step".to_string(), json!("old")),
        ]));
        let overlay = Context::from([
            ("_error_step".to_string(), json!("failed_step")),
            ("_error_message".to_string(), json!("boom")),
        ]);

        let execution_overlay = Context::from([(
            "_headers".to_string(),
            serde_json::json!({"x-signature": "secret"}),
        )]);
        let input = task_input_context(&shared, &execution_overlay, Some(&overlay));

        assert_eq!(input.get("shared"), Some(&json!(true)));
        assert_eq!(
            input.get("_headers"),
            Some(&serde_json::json!({"x-signature": "secret"}))
        );
        assert_eq!(input.get("_error_step"), Some(&json!("failed_step")));
        assert_eq!(input.get("_error_message"), Some(&json!("boom")));
        assert_eq!(shared.get("_error_step"), Some(&json!("old")));
        assert!(!shared.contains_key("_error_message"));
    }

    #[test]
    fn no_overlay_reuses_the_shared_snapshot() {
        let current = Arc::new(Context::from([("value".to_string(), json!(1))]));
        let input = task_input_context(&current, &Context::new(), None);

        assert!(Arc::ptr_eq(&input, &current));
    }
}
