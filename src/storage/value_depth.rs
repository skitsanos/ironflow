//! Store-side enforcement of the shared JSON nesting limit (IF-181).
//!
//! Every durable built-in store calls these before it writes, so a store
//! never holds a context or task record it cannot read back. The engine
//! refuses the same values earlier, as a request or step error; this is the
//! boundary for callers that use a store directly.

use crate::engine::types::{Context, TaskState};
use crate::storage::{StorageError, StorageResult};
use crate::util::json_depth;

pub(crate) fn admit_context(run_id: &str, ctx: &Context) -> StorageResult<()> {
    json_depth::check_entries(ctx).map_err(|error| {
        StorageError::invalid_input(format_args!(
            "Context for run '{run_id}' cannot be stored: {error}"
        ))
    })
}

pub(crate) fn admit_task(run_id: &str, task: &TaskState) -> StorageResult<()> {
    for (field, value) in [("input", &task.input), ("output", &task.output)] {
        let Some(value) = value else {
            continue;
        };
        json_depth::check_record(value).map_err(|error| {
            StorageError::invalid_input(format_args!(
                "Task '{}' {field} for run '{run_id}' cannot be stored: {error}",
                task.name
            ))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageErrorKind;
    use crate::util::json_depth::MAX_JSON_VALUE_DEPTH;

    fn arrays(depth: usize) -> serde_json::Value {
        (0..depth).fold(serde_json::Value::Null, |inner, _| {
            serde_json::Value::Array(vec![inner])
        })
    }

    #[test]
    fn a_context_over_the_limit_is_invalid_input_that_names_the_run_and_key() {
        let admitted = Context::from([("a".to_string(), arrays(MAX_JSON_VALUE_DEPTH))]);
        assert_eq!(admit_context("run-1", &admitted), Ok(()));

        let refused = Context::from([("a".to_string(), arrays(MAX_JSON_VALUE_DEPTH + 1))]);
        let error = admit_context("run-1", &refused).unwrap_err();
        assert_eq!(error.kind(), StorageErrorKind::InvalidInput);
        assert_eq!(
            error.to_string(),
            "Context for run 'run-1' cannot be stored: value 'a' is nested deeper than 123 \
             levels of arrays and objects"
        );
    }

    #[test]
    fn task_input_and_output_are_both_checked() {
        let mut task = TaskState::new("step", "code");
        assert_eq!(admit_task("run-1", &task), Ok(()));

        task.output = Some(serde_json::json!({ "v": arrays(MAX_JSON_VALUE_DEPTH) }));
        task.input = task.output.clone();
        assert_eq!(admit_task("run-1", &task), Ok(()));

        task.output = Some(serde_json::json!({ "v": arrays(MAX_JSON_VALUE_DEPTH + 1) }));
        let error = admit_task("run-1", &task).unwrap_err();
        assert_eq!(error.kind(), StorageErrorKind::InvalidInput);
        assert!(
            error
                .to_string()
                .starts_with("Task 'step' output for run 'run-1'")
        );

        task.output = None;
        task.input = Some(arrays(MAX_JSON_VALUE_DEPTH + 1));
        let error = admit_task("run-1", &task).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("Task 'step' input for run 'run-1'")
        );
    }
}
