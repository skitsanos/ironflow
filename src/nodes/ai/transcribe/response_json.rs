//! Streaming JSON admission for transcription provider responses.
//!
//! The HTTP byte ceiling bounds wire size, but a compact JSON array can expand
//! into substantially more heap when materialized as `serde_json::Value`.
//! Traverse the token stream first so depth and value-count ceilings are
//! authoritative before that allocation occurs.

use std::borrow::Cow;
use std::fmt;

use anyhow::Result;
use serde::Deserialize;
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};

const MAX_ERROR_JSON_BYTES: usize = 64 * 1024;
const OMITTED_ERROR_DETAIL: &str =
    "provider error body omitted: diagnostic JSON exceeds 65536 bytes";

pub(super) fn preflight_success(body: &str) -> Result<()> {
    preflight_with_limits(
        body,
        crate::util::limits::max_conversion_depth() as usize,
        crate::util::limits::max_conversion_nodes() as usize,
    )
    .map_err(|error| {
        anyhow::anyhow!(
            "transcribe: provider response could not be parsed as bounded JSON: {error}"
        )
    })
}

fn preflight_with_limits(body: &str, max_depth: usize, max_nodes: usize) -> Result<()> {
    let mut budget = JsonBudget {
        max_depth,
        max_nodes,
        nodes: 0,
    };
    let mut deserializer = serde_json::Deserializer::from_str(body);
    ValueSeed {
        budget: &mut budget,
        depth: 0,
    }
    .deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(())
}

struct JsonBudget {
    max_depth: usize,
    max_nodes: usize,
    nodes: usize,
}

impl JsonBudget {
    fn visit<E: serde::de::Error>(&mut self, depth: usize) -> std::result::Result<(), E> {
        if depth > self.max_depth {
            return Err(E::custom(format!(
                "maximum depth {} exceeded (raise IRONFLOW_MAX_CONVERSION_DEPTH)",
                self.max_depth
            )));
        }
        if self.nodes >= self.max_nodes {
            return Err(E::custom(format!(
                "maximum node count {} exceeded (raise IRONFLOW_MAX_CONVERSION_NODES)",
                self.max_nodes
            )));
        }
        self.nodes += 1;
        Ok(())
    }
}

struct ValueSeed<'a> {
    budget: &'a mut JsonBudget,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for ValueSeed<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        self.budget.visit(self.depth)?;
        deserializer.deserialize_any(ValueVisitor {
            budget: self.budget,
            depth: self.depth,
        })
    }
}

struct ValueVisitor<'a> {
    budget: &'a mut JsonBudget,
    depth: usize,
}

impl<'de> Visitor<'de> for ValueVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_none<E>(self) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> std::result::Result<(), E> {
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence
            .next_element_seed(ValueSeed {
                budget: self.budget,
                depth: self.depth + 1,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<(), A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_key::<IgnoredAny>()?.is_some() {
            map.next_value_seed(ValueSeed {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct BorrowedErrorBody<'a> {
    #[serde(borrow)]
    error: Option<BorrowedError<'a>>,
}

#[derive(Deserialize)]
struct BorrowedError<'a> {
    #[serde(borrow)]
    message: Option<&'a str>,
}

#[derive(Deserialize)]
struct OwnedErrorBody {
    error: Option<OwnedError>,
}

#[derive(Deserialize)]
struct OwnedError {
    message: Option<String>,
}

/// Extract the conventional `error.message` without materializing unknown
/// fields. Ordinary unescaped messages borrow directly from `body`; escaped
/// messages allocate at most the bounded diagnostic body size.
pub(super) fn provider_error_detail(body: &str) -> Cow<'_, str> {
    if body.len() > MAX_ERROR_JSON_BYTES {
        return Cow::Borrowed(OMITTED_ERROR_DETAIL);
    }

    if let Some(message) = serde_json::from_str::<BorrowedErrorBody<'_>>(body)
        .ok()
        .and_then(|parsed| parsed.error?.message)
    {
        return Cow::Borrowed(message);
    }

    serde_json::from_str::<OwnedErrorBody>(body)
        .ok()
        .and_then(|parsed| parsed.error?.message)
        .map(Cow::Owned)
        .unwrap_or_else(|| Cow::Borrowed(body.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_rejects_compact_node_amplification() {
        let body = serde_json::to_string(&vec![0; 20]).unwrap();
        let error = preflight_with_limits(&body, 64, 10)
            .unwrap_err()
            .to_string();
        assert!(error.contains("maximum node count 10"), "{error}");
        assert!(error.contains("IRONFLOW_MAX_CONVERSION_NODES"), "{error}");
    }

    #[test]
    fn preflight_limits_are_inclusive() {
        preflight_with_limits(r#"[["value"]]"#, 2, 3).unwrap();
    }

    #[test]
    fn preflight_rejects_excessive_nesting() {
        let error = preflight_with_limits(r#"[[["value"]]]"#, 2, 100)
            .unwrap_err()
            .to_string();
        assert!(error.contains("maximum depth 2"), "{error}");
        assert!(error.contains("IRONFLOW_MAX_CONVERSION_DEPTH"), "{error}");
    }

    #[test]
    fn provider_message_is_borrowed_and_unknown_fields_are_ignored() {
        let body = r#"{"padding":[1,2,3],"error":{"code":"bad","message":"denied"}}"#;
        let detail = provider_error_detail(body);
        assert_eq!(detail, "denied");
        assert!(matches!(detail, Cow::Borrowed(_)), "{detail:?}");
    }

    #[test]
    fn escaped_provider_message_uses_a_bounded_owned_value() {
        let detail = provider_error_detail(r#"{"error":{"message":"permission\ndenied"}}"#);
        assert!(matches!(detail, Cow::Owned(ref value) if value == "permission\ndenied"));
    }

    #[test]
    fn oversized_error_json_is_not_parsed_or_reflected() {
        let body = format!(
            r#"{{"error":{{"message":"{}"}}}}"#,
            "x".repeat(MAX_ERROR_JSON_BYTES)
        );
        assert_eq!(provider_error_detail(&body), OMITTED_ERROR_DETAIL);
    }
}
