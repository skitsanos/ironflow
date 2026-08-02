use serde_json::Value;

use super::{REDACTED, SecretRedactor};
use crate::engine::types::Context;

impl SecretRedactor {
    /// Redact an owned context without copying values that can be retained.
    /// The common no-overlay path returns immediately without traversing it.
    pub(crate) fn redact_context_owned(&self, mut context: Context) -> Context {
        if self.protected_keys.is_empty() && self.secrets.is_empty() {
            return context;
        }

        context.retain(|key, value| {
            if self.protected_keys.contains(key) || self.contains_secret(key) {
                return false;
            }
            self.redact_value_in_place(value);
            true
        });
        context
    }

    fn redact_value_in_place(&self, value: &mut Value) {
        match value {
            Value::String(text) if self.contains_secret(text) => {
                *text = self.redact_text_owned(std::mem::take(text));
            }
            Value::Array(items) => {
                for item in items {
                    self.redact_value_in_place(item);
                }
            }
            Value::Object(map) => map.retain(|key, value| {
                if self.protected_keys.contains(key) || self.contains_secret(key) {
                    return false;
                }
                self.redact_value_in_place(value);
                true
            }),
            Value::Number(number) if self.contains_exact_secret(&number.to_string()) => {
                *value = Value::String(REDACTED.to_string());
            }
            _ => {}
        }
    }

    fn redact_text_owned(&self, text: String) -> String {
        if !self.contains_secret(&text) {
            return text;
        }

        let mut redacted = text;
        for secret in self.secrets.iter() {
            if redacted == *secret {
                return REDACTED.to_string();
            }
            if redacted.contains(secret) {
                redacted = redacted.replace(secret, REDACTED);
            }
        }
        redacted
    }

    fn contains_secret(&self, text: &str) -> bool {
        self.secrets.iter().any(|secret| text.contains(secret))
    }

    fn contains_exact_secret(&self, text: &str) -> bool {
        self.secrets.iter().any(|secret| secret == text)
    }
}
