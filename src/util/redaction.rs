use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use crate::engine::types::Context;

mod owned;
mod tokens;

use tokens::SecretTokens;
pub(crate) use tokens::{MIN_SECRET_BYTES, meets_secret_floor};

pub(crate) const REDACTED: &str = "[REDACTED]";

/// Redacts invocation-only values before data crosses a durable, event, or
/// public-response boundary.
///
/// The overlay's own keys are removed at any depth. A complete overlay value,
/// and any component of it of at least [`MIN_SECRET_BYTES`], is replaced
/// wherever it occurs in a string, and an object key containing one is
/// dropped. Names, object keys and shorter components are not secrets
/// (IF-176). Record boundaries also replace a number that equals a secret;
/// the live boundary in `owned` keeps JSON types and engine routing keys.
#[derive(Clone, Debug, Default)]
pub(crate) struct SecretRedactor {
    protected_keys: Arc<HashSet<String>>,
    tokens: Arc<SecretTokens>,
}

impl SecretRedactor {
    pub(crate) fn from_overlay(overlay: &Context) -> Self {
        Self {
            protected_keys: Arc::new(overlay.keys().cloned().collect()),
            tokens: Arc::new(SecretTokens::from_values(overlay.values())),
        }
    }

    pub(crate) fn redact_context(&self, context: &Context) -> Context {
        context
            .iter()
            .filter(|(key, _)| !self.withholds_key(key))
            .map(|(key, value)| (key.clone(), self.redact_value(value)))
            .collect()
    }

    pub(crate) fn redact_value(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.redact_text(text)),
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| self.redact_value(item)).collect())
            }
            Value::Object(map) => Value::Object(
                map.iter()
                    .filter(|(key, _)| !self.withholds_key(key))
                    .map(|(key, value)| (key.clone(), self.redact_value(value)))
                    .collect(),
            ),
            Value::Number(number) if self.tokens.matches_number(number) => {
                Value::String(REDACTED.to_string())
            }
            other => other.clone(),
        }
    }

    pub(crate) fn redact_text(&self, text: &str) -> String {
        self.tokens.redact(text.to_string())
    }

    /// Whether a key is left out of scrubbed data: an overlay key, or a key
    /// whose own text holds a secret.
    pub(crate) fn withholds_key(&self, key: &str) -> bool {
        self.protected_keys.contains(key) || self.tokens.occur_in(key)
    }

    fn is_empty(&self) -> bool {
        self.protected_keys.is_empty() && self.tokens.is_empty()
    }
}

/// Defense-in-depth redaction for public/CLI serialization of legacy webhook
/// records. The historical `_headers` field supplies the exact values to
/// remove from the entire record, including renamed task outputs and errors.
/// Non-webhook runs are left untouched.
pub(crate) fn redact_legacy_webhook_record(value: &mut Value) {
    if let Value::Array(records) = value {
        for record in records {
            redact_legacy_webhook_record(record);
        }
        return;
    }
    let Some(Value::Object(headers)) = value
        .get("ctx")
        .and_then(|ctx| ctx.get("_headers"))
        .cloned()
    else {
        return;
    };
    let confidential_headers = headers
        .into_iter()
        .filter(|(name, _)| is_likely_confidential_header(name))
        .collect();
    let overlay = Context::from([("_headers".to_string(), Value::Object(confidential_headers))]);
    let redactor = SecretRedactor::from_overlay(&overlay);
    *value = redactor.redact_value(value);
}

fn is_likely_confidential_header(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase().replace('_', "-");
    normalized == "cookie"
        || normalized == "set-cookie"
        || [
            "api-key",
            "auth",
            "credential",
            "hmac",
            "jwt",
            "secret",
            "session",
            "signature",
            "token",
        ]
        .iter()
        .any(|marker| normalized.contains(marker))
}

#[cfg(test)]
mod tests;
