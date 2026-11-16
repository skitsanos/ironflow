//! Nested JSON builders for the IF-181 depth-limit suites.
//!
//! The depths are literals on purpose: these suites pin the public contract,
//! so a change to the shared limit has to be made here as well.

use serde_json::Value;

/// Deepest array/object nesting admitted for one context or step-output value.
// Each integration-test crate includes this shared module independently.
#[allow(dead_code)]
pub const ADMITTED_DEPTH: usize = 123;

/// The depth reported in IF-181: the parser admitted it, and the JSON and
/// Redis stores then wrote a run record they could not read back.
#[allow(dead_code)]
pub const REPORTED_DEPTH: usize = 126;

/// A value made of `depth` nested arrays.
#[allow(dead_code)]
pub fn nested_value(depth: usize) -> Value {
    let mut value = Value::Array(Vec::new());
    for _ in 1..depth {
        value = Value::Array(vec![value]);
    }
    value
}

/// JSON text for `depth` nested arrays.
#[allow(dead_code)]
pub fn nested_text(depth: usize) -> String {
    format!("{}{}", "[".repeat(depth), "]".repeat(depth))
}

/// A JSON object whose only key `a` holds `depth` nested arrays.
#[allow(dead_code)]
pub fn context_text(depth: usize) -> String {
    format!("{{\"a\":{}}}", nested_text(depth))
}
