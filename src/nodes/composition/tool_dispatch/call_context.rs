//! Build a handler's context from its `input` mapping.
//!
//! Strings are references. One into the tool call (`arguments`, `call`,
//! `tool_name`, `tool_call_id`) resolves to JSON null when the model did not
//! supply the value. One into the parent context (`ctx.<path>` or a bare key)
//! must exist: the flow author controls it, so its absence is a defect and
//! fails the step (IF-221).

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::engine::types::Context;

use super::super::input_mapping::{InputMappingError, Spec, classify, input_object};

fn scope(tool: &str) -> String {
    format!("tool_dispatch: tool '{tool}'")
}

fn missing(scope: &str, entry: &str) -> InputMappingError {
    InputMappingError::MissingSource {
        scope: scope.to_string(),
        entry: entry.to_string(),
    }
}

/// Check the mapping of every tool this batch calls. Parent references do
/// not depend on the call, so a missing one is refused before the first
/// handler runs.
pub(super) fn validate_inputs(
    tools: &Map<String, Value>,
    calls: &[Value],
    parent_ctx: &Context,
) -> Result<(), InputMappingError> {
    let mut checked = HashSet::new();
    for call in calls {
        let Some(name) = call.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(mapping) = tools.get(name) else {
            continue;
        };
        if !checked.insert(name) {
            continue;
        }
        let scope = scope(name);
        if let Some(input) = input_object(&scope, mapping)? {
            for (key, spec) in input {
                check_spec(&scope, key, spec, parent_ctx)?;
            }
        }
    }
    Ok(())
}

fn check_spec(
    scope: &str,
    entry: &str,
    spec: &Value,
    parent_ctx: &Context,
) -> Result<(), InputMappingError> {
    match classify(scope, entry, spec)? {
        Spec::Required(reference) => {
            if is_call_reference(reference) || parent_value(reference, parent_ctx).is_some() {
                Ok(())
            } else {
                Err(missing(scope, entry))
            }
        }
        Spec::Optional(_) | Spec::Literal(_) => Ok(()),
        Spec::Data(Value::Array(items)) => {
            items.iter().enumerate().try_for_each(|(index, item)| {
                check_spec(scope, &format!("{entry}.{index}"), item, parent_ctx)
            })
        }
        Spec::Data(Value::Object(fields)) => fields.iter().try_for_each(|(key, value)| {
            check_spec(scope, &format!("{entry}.{key}"), value, parent_ctx)
        }),
        Spec::Data(_) => Ok(()),
    }
}

pub(super) fn build_child_context(
    tool: &str,
    mapping: &Value,
    parent_ctx: &Context,
    call: &Value,
) -> Result<Context, InputMappingError> {
    let mut child_ctx = Context::new();
    child_ctx.insert("tool_call".to_string(), call.clone());
    child_ctx.insert(
        "tool_name".to_string(),
        call.get("name").cloned().unwrap_or(Value::Null),
    );
    child_ctx.insert(
        "tool_arguments".to_string(),
        call.get("arguments").cloned().unwrap_or(Value::Null),
    );
    child_ctx.insert(
        "tool_call_id".to_string(),
        call.get("id").cloned().unwrap_or(Value::Null),
    );
    child_ctx.insert(
        "tool_call_index".to_string(),
        call.get("index").cloned().unwrap_or(Value::Null),
    );

    let scope = scope(tool);
    if let Some(input) = input_object(&scope, mapping)? {
        for (key, spec) in input {
            let value = resolve_spec(&scope, key, spec, parent_ctx, call)?;
            child_ctx.insert(key.clone(), value);
        }
    }
    Ok(child_ctx)
}

fn resolve_spec(
    scope: &str,
    entry: &str,
    spec: &Value,
    parent_ctx: &Context,
    call: &Value,
) -> Result<Value, InputMappingError> {
    Ok(match classify(scope, entry, spec)? {
        Spec::Required(reference) => {
            resolve_reference(reference, parent_ctx, call).ok_or_else(|| missing(scope, entry))?
        }
        Spec::Optional(reference) => {
            resolve_reference(reference, parent_ctx, call).unwrap_or(Value::Null)
        }
        Spec::Literal(value) => value.clone(),
        Spec::Data(Value::Array(items)) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    resolve_spec(scope, &format!("{entry}.{index}"), item, parent_ctx, call)
                })
                .collect::<Result<_, _>>()?,
        ),
        Spec::Data(Value::Object(fields)) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| {
                    let path = format!("{entry}.{key}");
                    Ok((
                        key.clone(),
                        resolve_spec(scope, &path, value, parent_ctx, call)?,
                    ))
                })
                .collect::<Result<_, InputMappingError>>()?,
        ),
        Spec::Data(other) => other.clone(),
    })
}

/// `None` when the reference names a parent context value that is absent.
fn resolve_reference(reference: &str, parent_ctx: &Context, call: &Value) -> Option<Value> {
    match call_value(reference, call) {
        Some(value) => Some(value),
        None => parent_value(reference, parent_ctx).cloned(),
    }
}

fn is_call_reference(reference: &str) -> bool {
    matches!(
        reference,
        "arguments" | "call" | "tool_name" | "tool_call_id"
    ) || reference.starts_with("arguments.")
        || reference.starts_with("call.")
}

/// The value of a reference into the tool call, or `None` when the reference
/// addresses the parent context. A field the call does not carry is null.
fn call_value(reference: &str, call: &Value) -> Option<Value> {
    let arguments = || call.get("arguments").unwrap_or(&Value::Null);
    let value = if let Some(path) = reference.strip_prefix("arguments.") {
        resolve_path(arguments(), path)
    } else if let Some(path) = reference.strip_prefix("call.") {
        resolve_path(call, path)
    } else {
        match reference {
            "arguments" => Some(arguments()),
            "call" => Some(call),
            "tool_name" => call.get("name"),
            "tool_call_id" => call.get("id"),
            _ => return None,
        }
    };
    Some(value.cloned().unwrap_or(Value::Null))
}

/// `ctx.<key>[.<path>]` selects inside a parent key; any other string is a
/// parent key. Only an explicit root reference selects an entire object.
fn parent_value<'a>(reference: &str, parent_ctx: &'a Context) -> Option<&'a Value> {
    let Some(path) = reference.strip_prefix("ctx.") else {
        return parent_ctx.get(reference);
    };
    let (key, rest) = match path.split_once('.') {
        Some((key, rest)) => (key, Some(rest)),
        None => (path, None),
    };
    if key.is_empty() {
        return None;
    }
    let root = parent_ctx.get(key)?;
    match rest {
        None => Some(root),
        Some(rest) => resolve_path(root, rest),
    }
}

fn resolve_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return None;
    }
    let mut current = value;
    for part in path.split('.') {
        if part.is_empty() {
            return None;
        }
        current = if let Ok(index) = part.parse::<usize>() {
            current.as_array()?.get(index)?
        } else {
            current.get(part)?
        };
    }
    Some(current)
}

#[cfg(test)]
mod tests;
