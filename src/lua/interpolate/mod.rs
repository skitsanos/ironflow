//! Context interpolation for node configuration values.
//!
//! The `${ctx...}` namespace is intentionally a navigation grammar, not a
//! Lua expression evaluator. Parsing and rendering share the same path model
//! so flow validation cannot drift from runtime behavior.

mod path;
mod template;

use std::fmt;

use serde_json::Value;

use crate::engine::types::Context;

/// A malformed expression in IronFlow's reserved `${ctx...}` namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpolationError {
    expression: String,
    detail: String,
}

impl InterpolationError {
    pub(super) fn new(expression: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            expression: expression.into(),
            detail: detail.into(),
        }
    }

    /// The expression that failed validation.
    pub fn expression(&self) -> &str {
        &self.expression
    }

    /// A human-readable description of the grammar violation.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for InterpolationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid context interpolation `{}`: {}",
            self.expression, self.detail
        )
    }
}

impl std::error::Error for InterpolationError {}

/// Interpolate valid `${ctx...}` placeholders in one pass.
///
/// Supported selectors are dotted identifiers, zero-based array indexes, and
/// JSON-quoted object keys. Missing values and `null` retain the historical
/// behavior of resolving to an empty string. Non-`ctx` `${...}` forms are
/// preserved for downstream consumers such as a shell.
pub fn try_interpolate_ctx(
    template: &str,
    context: &Context,
) -> Result<String, InterpolationError> {
    template::render(template, context)
}

/// Interpolate `${ctx...}` placeholders, preserving a malformed template.
///
/// Evaluated flows reject malformed expressions during validation. This
/// compatibility wrapper remains infallible for callers that execute a node
/// directly; use [`try_interpolate_ctx`] when an error must be surfaced.
pub fn interpolate_ctx(template: &str, context: &Context) -> String {
    try_interpolate_ctx(template, context).unwrap_or_else(|_| template.to_string())
}

/// Recursively interpolate string values in a JSON value.
///
/// Array elements and object values are visited. Object keys remain
/// structural and are never interpolated.
pub fn interpolate_value(value: &Value, context: &Context) -> Value {
    match value {
        Value::String(template) => Value::String(interpolate_ctx(template, context)),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| interpolate_value(value, context))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), interpolate_value(value, context)))
                .collect(),
        ),
        value => value.clone(),
    }
}

/// Validate every string value in a parsed step configuration.
///
/// Paths are returned relative to `config`, using zero-based indexes for the
/// configuration tree as well as for context-array selectors.
pub(crate) fn validate_value(value: &Value) -> Vec<(String, InterpolationError)> {
    let mut errors = Vec::new();
    validate_value_at(value, "config", &mut errors);
    errors
}

fn validate_value_at(value: &Value, path: &str, errors: &mut Vec<(String, InterpolationError)>) {
    match value {
        Value::String(template) => {
            if let Err(error) = template::validate(template) {
                errors.push((path.to_string(), error));
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                validate_value_at(value, &format!("{path}[{index}]"), errors);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                let child_path = if path::is_identifier(key) {
                    format!("{path}.{key}")
                } else {
                    let quoted = serde_json::to_string(key)
                        .expect("serializing a JSON object key cannot fail");
                    format!("{path}[{quoted}]")
                };
                validate_value_at(value, &child_path, errors);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
