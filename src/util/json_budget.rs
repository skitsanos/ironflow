//! JSON text parsed into a `serde_json::Value` under a node budget (IF-215).
//!
//! A `Value` costs far more heap than its text: 40 MB of small array
//! elements becomes several hundred megabytes. The text is therefore read
//! once without building anything, counting values, and built only when the
//! count is within `IRONFLOW_MAX_JSON_PARSE_NODES`. The whole text is read
//! before the budget is judged, so a syntax error is still reported first.
//!
//! This is not the Lua conversion budget (`IRONFLOW_MAX_CONVERSION_NODES`):
//! a native node may hold a value that no Lua handler could convert, which
//! is what `context_keys` projection exists for (IF-136).

use std::fmt;

use serde::de::{DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

/// Why JSON text was not turned into a value.
#[derive(Debug)]
pub enum Refusal {
    /// The text is not valid JSON; the parser's own error.
    Syntax(serde_json::Error),
    /// The document holds more values than the budget admits.
    TooManyNodes { max: usize },
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(error) => error.fmt(formatter),
            Self::TooManyNodes { max } => write!(
                formatter,
                "JSON maximum node count {max} exceeded \
                 (raise IRONFLOW_MAX_JSON_PARSE_NODES to allow more)"
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// Parse `text` when it holds at most `IRONFLOW_MAX_JSON_PARSE_NODES`
/// values. Every scalar, array and object counts once; keys do not count.
pub fn parse(text: &str) -> Result<Value, Refusal> {
    parse_within(text, crate::util::limits::max_json_parse_nodes())
}

fn parse_within(text: &str, max: usize) -> Result<Value, Refusal> {
    if count_values(text).map_err(Refusal::Syntax)? > max {
        return Err(Refusal::TooManyNodes { max });
    }
    serde_json::from_str(text).map_err(Refusal::Syntax)
}

fn count_values(text: &str) -> Result<usize, serde_json::Error> {
    let mut values = 0;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    Count(&mut values).deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(values)
}

/// Visits one value and everything inside it, adding to the count.
struct Count<'count>(&'count mut usize);

impl Count<'_> {
    fn one<E>(self) -> Result<(), E> {
        *self.0 = self.0.saturating_add(1);
        Ok(())
    }
}

impl<'de> DeserializeSeed<'de> for Count<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Count<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        self.one()
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        self.one()
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        self.one()
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        self.one()
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        self.one()
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        self.one()
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        *self.0 = self.0.saturating_add(1);
        while seq.next_element_seed(Count(self.0))?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        *self.0 = self.0.saturating_add(1);
        while map.next_key::<IgnoredAny>()?.is_some() {
            map.next_value_seed(Count(self.0))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn every_scalar_array_and_object_counts_once_and_keys_do_not() {
        let text = r#"{"a":[1,2.5,"x",null,true],"b":{"c":-3}}"#;
        assert_eq!(count_values(text).unwrap(), 9);

        assert_eq!(
            parse_within(text, 9).unwrap(),
            json!({"a": [1, 2.5, "x", null, true], "b": {"c": -3}})
        );
        let refusal = parse_within(text, 8).unwrap_err();
        assert_eq!(
            refusal.to_string(),
            "JSON maximum node count 8 exceeded \
             (raise IRONFLOW_MAX_JSON_PARSE_NODES to allow more)"
        );
    }

    #[test]
    fn a_syntax_error_wins_over_the_budget_and_keeps_the_parser_message() {
        for text in ["[1,2,3", "[1,2,3] tail", "", "{\"a\":}"] {
            let expected = serde_json::from_str::<Value>(text).unwrap_err().to_string();
            let refusal = parse_within(text, 1).unwrap_err();
            assert!(matches!(refusal, Refusal::Syntax(_)), "{text}");
            assert_eq!(refusal.to_string(), expected, "{text}");
        }
    }
}
