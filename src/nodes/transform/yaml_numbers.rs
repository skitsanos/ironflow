//! Numbers that YAML can spell but a JSON value cannot hold (IF-223).
//!
//! The parser resolves `.nan`, `.inf`, an overflowing exponent and an
//! integer outside the signed 64-bit range to a float, which is `null` in
//! JSON for the first three and a rounded value for the last. Such a scalar
//! is kept as its source text instead, the rule `csv_parse` follows
//! (IF-196). The resolved value no longer carries that text, so a document
//! that holds one is read a second time with scalar resolution off and the
//! two trees are walked together.

use anyhow::{Result, bail};
use noyalib as yaml;
use serde_json::Value;

/// The magnitude of the first integer past the signed 64-bit range, 2^63.
const INTEGER_LIMIT: f64 = 9_223_372_036_854_775_808.0;

/// Parse YAML text into a JSON value without altering any number.
pub(super) fn parse(text: &str) -> Result<Value> {
    let resolved: yaml::Value = yaml::from_str(text)?;
    let source = if holds_inexact(&resolved) {
        let config = yaml::ParserConfig::new().no_schema(true);
        Some(yaml::from_str_with_config::<yaml::Value>(text, &config)?)
    } else {
        None
    };
    to_json(resolved, source)
}

/// The first integer in `value` that the YAML writer cannot write exactly.
pub(super) fn unwritable_integer(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) if !number.is_i64() => number.as_u64(),
        Value::Array(values) => values.iter().find_map(unwritable_integer),
        Value::Object(map) => map.values().find_map(unwritable_integer),
        _ => None,
    }
}

/// Whether a float may stand for a scalar that was not written as one.
fn inexact(number: f64) -> bool {
    !number.is_finite() || (number.fract() == 0.0 && number.abs() >= INTEGER_LIMIT)
}

fn holds_inexact(value: &yaml::Value) -> bool {
    match value {
        yaml::Value::Number(number) => number.as_i64().is_none() && inexact(number.as_f64()),
        yaml::Value::Sequence(items) => items.iter().any(holds_inexact),
        yaml::Value::Mapping(map) => map.iter().any(|(_, value)| holds_inexact(value)),
        yaml::Value::Tagged(tagged) => holds_inexact(tagged.value()),
        _ => false,
    }
}

/// Convert `value`; `source` is the same node with every plain scalar left
/// as text, present only when the document holds an inexact number.
fn to_json(value: yaml::Value, source: Option<yaml::Value>) -> Result<Value> {
    let source = source.map(untagged);
    Ok(match value {
        yaml::Value::Null => Value::Null,
        yaml::Value::Bool(boolean) => Value::Bool(boolean),
        yaml::Value::Number(number) => match number.as_i64() {
            Some(integer) => Value::Number(integer.into()),
            None => float_to_json(number.as_f64(), source)?,
        },
        yaml::Value::String(text) => Value::String(text),
        yaml::Value::Sequence(items) => {
            let mut sources = match source {
                Some(yaml::Value::Sequence(sources)) if sources.len() == items.len() => {
                    Some(sources.into_iter())
                }
                _ => None,
            };
            let mut array = Vec::with_capacity(items.len());
            for item in items {
                let source = sources.as_mut().and_then(Iterator::next);
                array.push(to_json(item, source)?);
            }
            Value::Array(array)
        }
        yaml::Value::Mapping(map) => {
            let mut sources = match source {
                Some(yaml::Value::Mapping(sources)) if sources.len() == map.len() => {
                    Some(sources.into_iter())
                }
                _ => None,
            };
            let mut object = serde_json::Map::new();
            for (key, value) in map {
                let source = sources.as_mut().and_then(Iterator::next);
                object.insert(key, to_json(value, source.map(|(_, value)| value))?);
            }
            Value::Object(object)
        }
        yaml::Value::Tagged(tagged) => to_json(tagged.into_parts().1, source)?,
    })
}

fn untagged(mut value: yaml::Value) -> yaml::Value {
    while let yaml::Value::Tagged(tagged) = value {
        value = tagged.into_parts().1;
    }
    value
}

/// A float is a JSON number when it is finite and was written as a float.
/// An integer literal that only fits a float, and any non-finite spelling,
/// becomes its source text. Without source text (an explicit `!!float` tag)
/// a finite value is what the document asked for and a non-finite one fails.
fn float_to_json(number: f64, source: Option<yaml::Value>) -> Result<Value> {
    if !inexact(number) {
        return finite(number);
    }
    match source {
        Some(yaml::Value::String(text)) if !number.is_finite() || integer_literal(&text) => {
            Ok(Value::String(text))
        }
        Some(yaml::Value::String(_) | yaml::Value::Number(_)) => finite(number),
        _ => bail!(
            "yaml_parse: a scalar resolves to {number} and its source text could not be \
             located; write it as a quoted string"
        ),
    }
}

fn finite(number: f64) -> Result<Value> {
    match serde_json::Number::from_f64(number) {
        Some(number) => Ok(Value::Number(number)),
        None => bail!(
            "yaml_parse: a scalar resolves to {number}, which is not a finite number; \
             write it as a quoted string"
        ),
    }
}

fn integer_literal(text: &str) -> bool {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}
