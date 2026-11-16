use crate::engine::types::Context;

use super::comparison::{Comparison, ComparisonError, context_path};

/// A guard condition: `<path> <operator> <literal>`, `<path> exists`, or a
/// bare `<path>` tested for truthiness. Anything else is rejected.
enum Condition<'a> {
    Exists(&'a str),
    Compare(Comparison<'a>),
    Truthy(&'a str),
}

impl<'a> Condition<'a> {
    fn parse(condition: &'a str) -> Result<Self, ComparisonError> {
        let condition = condition.trim();
        if condition.ends_with(" exists") {
            let key = condition.trim_end_matches(" exists").trim();
            return context_path(key).map(Self::Exists);
        }
        match Comparison::parse(condition)? {
            Some(comparison) => Ok(Self::Compare(comparison)),
            None => context_path(condition).map(Self::Truthy),
        }
    }
}

/// Evaluate a guard condition against context. A condition outside the
/// grammar, or a comparison with no defined result, is an error.
pub(super) fn evaluate_condition(condition: &str, ctx: &Context) -> Result<bool, ComparisonError> {
    Ok(match Condition::parse(condition)? {
        Condition::Exists(path) => resolve_nested(path, ctx).is_some(),
        Condition::Compare(comparison) => {
            return comparison.evaluate(resolve_nested(strip_ctx_prefix(comparison.left), ctx));
        }
        Condition::Truthy(path) => match resolve_nested(path, ctx) {
            Some(serde_json::Value::Bool(value)) => *value,
            Some(serde_json::Value::Null) | None => false,
            Some(_) => true,
        },
    })
}

/// Reject a condition that is invalid whatever the context holds.
pub(super) fn validate_condition(condition: &str) -> Result<(), ComparisonError> {
    Condition::parse(condition).map(|_| ())
}

pub(super) fn resolve_nested<'a>(path: &str, ctx: &'a Context) -> Option<&'a serde_json::Value> {
    let mut parts = path.split('.');
    let mut current = ctx.get(parts.next()?)?;
    for part in parts {
        current = current.get(part)?;
    }
    Some(current)
}

pub(super) fn resolve_ctx_value(expression: &str, ctx: &Context) -> String {
    match resolve_nested(strip_ctx_prefix(expression), ctx) {
        Some(serde_json::Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

fn strip_ctx_prefix(value: &str) -> &str {
    value.strip_prefix("ctx.").unwrap_or(value)
}
