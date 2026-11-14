//! Value equality for `data_filter` and `deduplicate` (IF-196).
//!
//! `serde_json` keeps integers and floats apart, so `5 == 5.0` is false
//! there, and Lua arithmetic such as `10 / 2` produces the float. These
//! helpers compare numbers by value and leave every other type strict: a
//! string never equals a number or a boolean.

use serde_json::{Number, Value};

/// Structural equality in which numbers are equal when their values are.
pub(super) fn json_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => number_equal(left, right),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| json_equal(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right.get(key).is_some_and(|other| json_equal(value, other))
                })
        }
        _ => left == right,
    }
}

/// A string that is the same for values `json_equal` treats as equal.
pub(super) fn canonical_json(value: &Value) -> String {
    canonical(value).to_string()
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Number(number) if number.is_f64() => whole(number)
            .and_then(|whole| {
                i64::try_from(whole)
                    .map(Number::from)
                    .or_else(|_| u64::try_from(whole).map(Number::from))
                    .ok()
            })
            .map_or_else(|| value.clone(), Value::Number),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), canonical(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

/// Integers compare exactly, so two large integers that share a nearest
/// float stay distinct; a float equals an integer only when it has no
/// fractional part and the same value.
fn number_equal(left: &Number, right: &Number) -> bool {
    match (whole(left), whole(right)) {
        (Some(left), Some(right)) => left == right,
        (None, None) => left.as_f64() == right.as_f64(),
        _ => false,
    }
}

/// The exact value of a number that has no fractional part.
fn whole(number: &Number) -> Option<i128> {
    /// Every float below this magnitude converts to `i128` without loss.
    const LIMIT: f64 = 1e30;
    if let Some(value) = number.as_i64() {
        return Some(value.into());
    }
    if let Some(value) = number.as_u64() {
        return Some(value.into());
    }
    let value = number.as_f64()?;
    #[allow(clippy::cast_possible_truncation)]
    (value.fract() == 0.0 && value.abs() < LIMIT).then_some(value as i128)
}
