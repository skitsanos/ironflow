//! The `input` mapping contract shared by the composition nodes (IF-221).
//!
//! A string names a parent context value that must exist. Two single-key
//! objects state the exceptions: `{ optional = "<reference>" }` may be absent
//! and `{ literal = <value> }` is passed as written. Anything else is data.

use std::fmt;

use serde_json::{Map, Value};

use crate::engine::types::Context;

const HINT: &str = "use { optional = \"<key>\" } when it may be absent \
                    or { literal = <value> } to pass a literal";

/// Why an `input` mapping was refused. The message names the mapping entry
/// and never the configured reference or a context value, because a string
/// that used to fall back to a literal may hold anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMappingError {
    /// The entry references a parent context value that does not exist.
    MissingSource { scope: String, entry: String },
    /// `{ optional = ... }` does not hold a non-empty string.
    InvalidOptional { scope: String, entry: String },
    /// `input` is present and is not an object.
    NotAnObject { scope: String },
}

impl fmt::Display for InputMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSource { scope, entry } => write!(
                formatter,
                "{scope}: input '{entry}' references a parent context value that does not \
                 exist; {HINT}"
            ),
            Self::InvalidOptional { scope, entry } => write!(
                formatter,
                "{scope}: input '{entry}' has an 'optional' form that is not a non-empty \
                 string naming the parent context value"
            ),
            Self::NotAnObject { scope } => {
                write!(formatter, "{scope}: 'input' must be an object")
            }
        }
    }
}

impl std::error::Error for InputMappingError {}

/// One mapping value, classified.
pub(super) enum Spec<'a> {
    /// A string: a reference that must resolve.
    Required(&'a str),
    /// `{ optional = "<reference>" }`.
    Optional(&'a str),
    /// `{ literal = <value> }`.
    Literal(&'a Value),
    /// Any other value. The subworkflow nodes pass it as written;
    /// `tool_dispatch` resolves the strings nested inside it.
    Data(&'a Value),
}

pub(super) fn classify<'a>(
    scope: &str,
    entry: &str,
    spec: &'a Value,
) -> Result<Spec<'a>, InputMappingError> {
    let fields = match spec {
        Value::String(reference) => return Ok(Spec::Required(reference)),
        Value::Object(fields) if fields.len() == 1 => fields,
        other => return Ok(Spec::Data(other)),
    };
    if let Some(literal) = fields.get("literal") {
        return Ok(Spec::Literal(literal));
    }
    match fields.get("optional") {
        Some(Value::String(reference)) if !reference.is_empty() => Ok(Spec::Optional(reference)),
        Some(_) => Err(InputMappingError::InvalidOptional {
            scope: scope.to_string(),
            entry: entry.to_string(),
        }),
        None => Ok(Spec::Data(spec)),
    }
}

/// The `input` object of a node or flow entry, or `None` when it is unset.
pub(super) fn input_object<'a>(
    scope: &str,
    config: &'a Value,
) -> Result<Option<&'a Map<String, Value>>, InputMappingError> {
    match config.get("input") {
        None => Ok(None),
        Some(Value::Object(input)) => Ok(Some(input)),
        Some(_) => Err(InputMappingError::NotAnObject {
            scope: scope.to_string(),
        }),
    }
}

/// Resolve one entry of a parent-key mapping without copying the value.
/// `Ok(None)` is an optional entry whose parent key is absent.
fn parent_entry<'a>(
    scope: &str,
    entry: &str,
    spec: &'a Value,
    parent: &'a Context,
) -> Result<Option<&'a Value>, InputMappingError> {
    match classify(scope, entry, spec)? {
        Spec::Required(key) => {
            parent
                .get(key)
                .map(Some)
                .ok_or_else(|| InputMappingError::MissingSource {
                    scope: scope.to_string(),
                    entry: entry.to_string(),
                })
        }
        Spec::Optional(key) => Ok(parent.get(key)),
        Spec::Literal(value) | Spec::Data(value) => Ok(Some(value)),
    }
}

/// Check a parent-key mapping without building the child context, so a node
/// can refuse it before any child starts.
pub(super) fn validate_parent_input(
    scope: &str,
    input: &Map<String, Value>,
    parent: &Context,
) -> Result<(), InputMappingError> {
    for (entry, spec) in input {
        parent_entry(scope, entry, spec, parent)?;
    }
    Ok(())
}

/// Build a child context from a parent-key mapping: `child_key = "parent_key"`.
pub(super) fn map_parent_input(
    scope: &str,
    input: &Map<String, Value>,
    parent: &Context,
) -> Result<Context, InputMappingError> {
    let mut child = Context::new();
    for (entry, spec) in input {
        if let Some(value) = parent_entry(scope, entry, spec, parent)? {
            child.insert(entry.clone(), value.clone());
        }
    }
    Ok(child)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn parent() -> Context {
        Context::from([("present".into(), json!(1)), ("null".into(), Value::Null)])
    }

    fn map(input: Value) -> Result<Context, InputMappingError> {
        map_parent_input("node", input.as_object().unwrap(), &parent())
    }

    #[test]
    fn a_missing_key_is_a_typed_error_that_names_only_the_entry() {
        let error = map(json!({"child": "absent-reference"})).unwrap_err();
        assert_eq!(
            error,
            InputMappingError::MissingSource {
                scope: "node".into(),
                entry: "child".into()
            }
        );
        assert!(!error.to_string().contains("absent-reference"));
        let through_anyhow = anyhow::Error::new(error);
        assert!(through_anyhow.downcast_ref::<InputMappingError>().is_some());
    }

    #[test]
    fn a_key_holding_null_exists() {
        assert_eq!(map(json!({"child": "null"})).unwrap()["child"], Value::Null);
    }

    #[test]
    fn only_exact_single_key_objects_are_forms() {
        let data = json!({"optional": "present", "other": true});
        let child = map(json!({
            "a": {"optional": "absent"}, "b": {"optional": "present"},
            "c": {"literal": "present"}, "d": data, "e": {"literal": {"literal": 2}},
            "f": {"key": "present"}, "g": ["present"], "h": false
        }))
        .unwrap();
        assert!(!child.contains_key("a"));
        assert_eq!(child["b"], 1);
        assert_eq!(child["c"], "present");
        assert_eq!(child["d"], data);
        assert_eq!(child["e"], json!({"literal": 2}));
        assert_eq!(child["f"], json!({"key": "present"}));
        assert_eq!(child["g"], json!(["present"]));
        assert_eq!(child["h"], false);
    }

    #[test]
    fn validation_agrees_with_mapping() {
        for input in [
            json!({"child": "absent"}),
            json!({"child": {"optional": 1}}),
            json!({"child": {"optional": ""}}),
            json!({"child": "present"}),
        ] {
            let input = input.as_object().unwrap();
            assert_eq!(
                validate_parent_input("node", input, &parent()).err(),
                map_parent_input("node", input, &parent()).err()
            );
        }
    }
}
