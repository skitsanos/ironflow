//! Derives the secret text of invocation-only values (IF-011, IF-176).
//!
//! A complete value is always protected. A component extracted from it is a
//! secret only at or above [`MIN_SECRET_BYTES`], and names and object keys
//! never are: they describe the value instead of carrying it, and a short
//! component such as `true` or `1` cannot be told apart from ordinary data.
//!
//! The floor exists because overlay values are text a remote party chose. A
//! credential the caller resolved itself (IF-108) is a secret whatever its
//! length, so [`SecretTokens::from_credentials`] does not apply the floor to
//! the complete value or to the credential after its authentication scheme.

use std::cmp::Reverse;
use std::collections::HashSet;

use serde_json::{Number, Value};

use super::REDACTED;

/// Shortest text that is redacted wherever it occurs. A webhook rejects a
/// forwarded header below this floor.
pub(crate) const MIN_SECRET_BYTES: usize = 8;

/// Whether `text` has enough non-whitespace bytes to be matched as a substring.
pub(crate) fn meets_secret_floor(text: &str) -> bool {
    text.bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .count()
        >= MIN_SECRET_BYTES
}

/// Secret text derived from a set of invocation-only values.
#[derive(Debug, Default)]
pub(super) struct SecretTokens {
    /// Redacted wherever they occur: overlay text at or above the floor and
    /// resolved credentials of any length. Longest first, so a whole value
    /// is replaced before one of its components.
    fragments: Vec<String>,
    /// Complete values below the floor: only an identical value matches.
    exact: HashSet<String>,
    /// The tokens a JSON number can equal.
    numeric: HashSet<String>,
}

impl SecretTokens {
    /// Secrets of invocation-only values, such as forwarded webhook headers.
    pub(super) fn from_values<'a>(values: impl IntoIterator<Item = &'a Value>) -> Self {
        let mut collector = Collector::default();
        for value in values {
            collector.value(value);
        }
        collector.finish()
    }

    /// Secrets of credentials the caller resolved itself. Each one, and the
    /// credential after its `Bearer` or `Basic` scheme, is redacted wherever
    /// it occurs, however short it is.
    pub(super) fn from_credentials<'a>(credentials: impl IntoIterator<Item = &'a str>) -> Self {
        let mut collector = Collector {
            resolved_credentials: true,
            ..Collector::default()
        };
        for credential in credentials {
            collector.text(credential);
        }
        collector.finish()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.fragments.is_empty() && self.exact.is_empty()
    }

    /// Whether `text` holds a secret: a fragment anywhere, or a short value
    /// as the entire text.
    pub(super) fn occur_in(&self, text: &str) -> bool {
        self.exact.contains(text.trim())
            || self
                .fragments
                .iter()
                .any(|fragment| text.contains(fragment.as_str()))
    }

    pub(super) fn matches_number(&self, number: &Number) -> bool {
        !self.numeric.is_empty() && self.numeric.contains(&number.to_string())
    }

    /// Replace every secret in `text`.
    pub(super) fn redact(&self, text: String) -> String {
        if self.exact.contains(text.trim()) {
            return REDACTED.to_string();
        }
        let mut redacted = text;
        for fragment in &self.fragments {
            if redacted == *fragment {
                return REDACTED.to_string();
            }
            if redacted.contains(fragment.as_str()) {
                redacted = redacted.replace(fragment.as_str(), REDACTED);
            }
        }
        redacted
    }
}

#[derive(Default)]
struct Collector {
    fragments: HashSet<String>,
    exact: HashSet<String>,
    /// The values are credentials the caller resolved, not text a remote
    /// party chose, so a short one is still matched as a substring.
    resolved_credentials: bool,
}

impl Collector {
    fn value(&mut self, value: &Value) {
        match value {
            Value::String(text) => self.text(text),
            Value::Number(number) => self.whole(&number.to_string()),
            Value::Array(items) => items.iter().for_each(|item| self.value(item)),
            // Names describe a value; only the values are confidential.
            Value::Object(map) => map.values().for_each(|value| self.value(value)),
            Value::Bool(_) | Value::Null => {}
        }
    }

    fn text(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.whole(text);

        if let Ok(structured) = serde_json::from_str::<Value>(text) {
            self.structured(&structured);
        }

        // Signature formats commonly use comma/semicolon-delimited key=value
        // components. Track long component values too, so extracting `v1`
        // does not evade the persistence fence.
        for component in text.split([',', ';']) {
            if let Some((_, value)) = component.trim().split_once('=') {
                self.component(value.trim().trim_matches('"').trim());
            }
        }

        if let Some((scheme, credential)) = text.split_once(char::is_whitespace)
            && matches!(scheme.to_ascii_lowercase().as_str(), "bearer" | "basic")
        {
            self.scheme_credential(credential.trim());
        }
    }

    fn structured(&mut self, value: &Value) {
        match value {
            Value::String(text) => self.component(text.trim()),
            Value::Number(number) => self.component(&number.to_string()),
            Value::Array(items) => items.iter().for_each(|item| self.structured(item)),
            Value::Object(map) => map.values().for_each(|value| self.structured(value)),
            Value::Bool(_) | Value::Null => {}
        }
    }

    /// A complete value is never ignored. An overlay value below the floor
    /// can only match exactly, so it cannot splice a marker into unrelated
    /// text; a resolved credential is matched as a substring at any length.
    fn whole(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.resolved_credentials || meets_secret_floor(text) {
            self.fragments.insert(text.to_string());
        } else {
            self.exact.insert(text.to_string());
        }
    }

    /// The credential after a `Bearer` or `Basic` scheme.
    fn scheme_credential(&mut self, text: &str) {
        if self.resolved_credentials && !text.is_empty() {
            self.fragments.insert(text.to_string());
        } else {
            self.component(text);
        }
    }

    fn component(&mut self, text: &str) {
        if meets_secret_floor(text) {
            self.fragments.insert(text.to_string());
        }
    }

    fn finish(self) -> SecretTokens {
        let mut fragments = self.fragments;
        // A secret embedded in serialized JSON appears in its escaped form.
        let escaped = fragments
            .iter()
            .filter_map(|fragment| serde_json::to_string(fragment).ok())
            .filter_map(|encoded| {
                encoded
                    .strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        fragments.extend(escaped);

        let numeric = fragments
            .iter()
            .chain(&self.exact)
            .filter(|token| serde_json::from_str::<Number>(token).is_ok())
            .cloned()
            .collect();
        let mut fragments = fragments.into_iter().collect::<Vec<_>>();
        fragments.sort_unstable_by(|left, right| {
            Reverse(left.len())
                .cmp(&Reverse(right.len()))
                .then_with(|| left.cmp(right))
        });

        SecretTokens {
            fragments,
            exact: self.exact,
            numeric,
        }
    }
}
