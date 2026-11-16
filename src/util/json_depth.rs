//! One nesting limit for the JSON that IronFlow accepts and persists (IF-181).
//!
//! `serde_json` refuses to parse the 128th nested array or object. A stored
//! run record wraps every context and task-output value in further objects,
//! so a value the parser admitted on its own could be written and then never
//! read back. Input parsing, the engine and the state stores share this limit
//! instead: what one of them admits, the others can store and return.

use std::fmt;

use serde_json::Value;

/// Nested arrays and objects `serde_json` parses before it reports
/// `recursion limit exceeded`.
const PARSER_NESTING: usize = 127;

/// Objects a stored run record puts around one task-output value: the run,
/// its task map, the task and the output object. A context value sits under
/// two (the run and its context), so the task record sets the limit.
const RECORD_ENVELOPE: usize = 4;

/// Deepest nesting of arrays and objects admitted for one context value or
/// one step-output value. A scalar has depth 0 and `[[1]]` has depth 2.
pub const MAX_JSON_VALUE_DEPTH: usize = PARSER_NESTING - RECORD_ENVELOPE;

/// Characters of an offending key repeated in the error.
const REPORTED_KEY_CHARS: usize = 64;

/// A value nested deeper than [`MAX_JSON_VALUE_DEPTH`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthExceeded {
    key: Option<String>,
}

impl DepthExceeded {
    /// The refusal for one value, for a parser that tracks nesting while it
    /// reads instead of checking a finished value.
    pub(crate) fn value() -> Self {
        Self { key: None }
    }
}

impl fmt::Display for DepthExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.key {
            Some(key) => write!(formatter, "value '{key}' is ")?,
            None => formatter.write_str("JSON value is ")?,
        }
        write!(
            formatter,
            "nested deeper than {MAX_JSON_VALUE_DEPTH} levels of arrays and objects"
        )
    }
}

impl std::error::Error for DepthExceeded {}

/// Check one value, such as the result of parsing JSON text.
pub fn check_value(value: &Value) -> Result<(), DepthExceeded> {
    if exceeds(value, MAX_JSON_VALUE_DEPTH) {
        return Err(DepthExceeded::value());
    }
    Ok(())
}

/// Check every value of a context or step output. The error names the
/// offending key, the first in sorted order when several are too deep.
pub fn check_entries<'a, I>(entries: I) -> Result<(), DepthExceeded>
where
    I: IntoIterator<Item = (&'a String, &'a Value)>,
{
    let offending = entries
        .into_iter()
        .filter(|(_, value)| exceeds(value, MAX_JSON_VALUE_DEPTH))
        .map(|(key, _)| key)
        .min();
    match offending {
        Some(key) => Err(DepthExceeded {
            key: Some(reported_key(key)),
        }),
        None => Ok(()),
    }
}

/// Check a task's recorded input or output: an object is checked per key,
/// like the context it is published into.
pub fn check_record(value: &Value) -> Result<(), DepthExceeded> {
    match value {
        Value::Object(entries) => check_entries(entries),
        other => check_value(other),
    }
}

/// Whether `value` nests more than `remaining` arrays and objects. Recursion
/// stops one level past the limit, so a hostile value cannot exhaust the stack.
fn exceeds(value: &Value, remaining: usize) -> bool {
    match value {
        Value::Array(items) => {
            remaining == 0 || items.iter().any(|item| exceeds(item, remaining - 1))
        }
        Value::Object(entries) => {
            remaining == 0 || entries.values().any(|item| exceeds(item, remaining - 1))
        }
        _ => false,
    }
}

/// A key as it appears in an error: bounded, with control characters escaped.
fn reported_key(key: &str) -> String {
    let mut reported: String = key
        .chars()
        .take(REPORTED_KEY_CHARS)
        .flat_map(char::escape_debug)
        .collect();
    if key.chars().nth(REPORTED_KEY_CHARS).is_some() {
        reported.push_str("...");
    }
    reported
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::Utc;

    use super::*;
    use crate::engine::types::{RunInfo, RunStatus, TaskState};

    fn arrays(depth: usize) -> Value {
        (0..depth).fold(Value::from(1), |inner, _| Value::Array(vec![inner]))
    }

    fn objects(depth: usize) -> Value {
        (0..depth).fold(Value::from(1), |inner, _| serde_json::json!({ "k": inner }))
    }

    fn run_with(context_depth: usize, output_depth: usize) -> RunInfo {
        let mut task = TaskState::new("step", "json_parse");
        task.output = Some(serde_json::json!({ "parsed": arrays(output_depth) }));
        RunInfo {
            id: "run".to_string(),
            flow_name: "flow".to_string(),
            status: RunStatus::Running,
            started: Some(Utc::now()),
            finished: None,
            ctx: HashMap::from([("a".to_string(), arrays(context_depth))]),
            tasks: HashMap::from([("step".to_string(), task)]),
        }
    }

    #[test]
    fn the_limit_is_what_a_stored_task_output_leaves_of_the_parser_limit() {
        assert_eq!(MAX_JSON_VALUE_DEPTH, 123);
    }

    #[test]
    fn values_at_the_limit_pass_and_one_level_deeper_fails() {
        for build in [arrays, objects] {
            assert_eq!(check_value(&build(MAX_JSON_VALUE_DEPTH)), Ok(()));
            assert!(check_value(&build(MAX_JSON_VALUE_DEPTH + 1)).is_err());
        }
        assert_eq!(check_value(&Value::Null), Ok(()));
        assert_eq!(check_value(&serde_json::json!([[], {}, "text"])), Ok(()));
    }

    #[test]
    fn a_deep_branch_is_found_behind_shallow_siblings() {
        let value = serde_json::json!([1, { "shallow": [] }, arrays(MAX_JSON_VALUE_DEPTH)]);
        assert!(check_value(&value).is_err());
    }

    #[test]
    fn far_deeper_values_are_refused_without_walking_them() {
        // Built iteratively and leaked: dropping it would recurse 100,000 deep.
        let value = std::mem::ManuallyDrop::new(arrays(100_000));
        assert!(check_value(&value).is_err());
    }

    #[test]
    fn entries_name_the_first_offending_key_in_sorted_order() {
        let context = HashMap::from([
            ("fine".to_string(), arrays(MAX_JSON_VALUE_DEPTH)),
            ("zeta".to_string(), arrays(MAX_JSON_VALUE_DEPTH + 1)),
            ("beta".to_string(), arrays(MAX_JSON_VALUE_DEPTH + 3)),
        ]);
        let error = check_entries(&context).unwrap_err();
        assert_eq!(
            error.to_string(),
            "value 'beta' is nested deeper than 123 levels of arrays and objects"
        );
        assert_eq!(check_entries(&HashMap::new()), Ok(()));
    }

    #[test]
    fn reported_keys_are_bounded_and_escaped() {
        let key = format!("line\nbreak{}", "x".repeat(200));
        let context = HashMap::from([(key, arrays(MAX_JSON_VALUE_DEPTH + 1))]);
        let message = check_entries(&context).unwrap_err().to_string();
        assert!(message.starts_with("value 'line\\nbreakxxx"), "{message}");
        assert!(message.contains("...' is nested deeper"), "{message}");
        assert!(message.len() < 200, "{message}");
    }

    #[test]
    fn a_record_checks_object_entries_and_bare_values() {
        let object = serde_json::json!({ "parsed": arrays(MAX_JSON_VALUE_DEPTH) });
        assert_eq!(check_record(&object), Ok(()));
        let deep = serde_json::json!({ "parsed": arrays(MAX_JSON_VALUE_DEPTH + 1) });
        assert_eq!(
            check_record(&deep).unwrap_err().to_string(),
            "value 'parsed' is nested deeper than 123 levels of arrays and objects"
        );
        assert!(check_record(&arrays(MAX_JSON_VALUE_DEPTH + 1)).is_err());
    }

    /// The limit is exactly what a serialized run record can be read back
    /// with: one level more in a task output and `serde_json` refuses it.
    #[test]
    fn a_run_record_at_the_limit_parses_back_and_one_level_deeper_does_not() {
        let admitted = run_with(MAX_JSON_VALUE_DEPTH, MAX_JSON_VALUE_DEPTH);
        let text = serde_json::to_string(&admitted).unwrap();
        let read: RunInfo = serde_json::from_str(&text).unwrap();
        assert!(read.ctx == admitted.ctx);
        assert!(read.tasks["step"].output == admitted.tasks["step"].output);

        let unreadable = run_with(MAX_JSON_VALUE_DEPTH, MAX_JSON_VALUE_DEPTH + 1);
        let text = serde_json::to_string(&unreadable).unwrap();
        let error = serde_json::from_str::<RunInfo>(&text).unwrap_err();
        assert!(error.to_string().contains("recursion limit"), "{error}");
    }
}
