use serde_json::Value;

use super::{REDACTED, SecretRedactor};
use crate::engine::types::Context;

impl SecretRedactor {
    /// Redact an owned context for a record without copying values that can
    /// be retained. The common no-overlay path returns immediately.
    ///
    /// There is no form of this for data that stays in a running workflow:
    /// redaction applies only where IronFlow records or returns a value
    /// (IF-219).
    pub(crate) fn redact_context_owned(&self, mut context: Context) -> Context {
        if self.is_empty() {
            return context;
        }

        context.retain(|key, value| {
            if self.withholds_key(key) {
                return false;
            }
            self.scrub_value(value);
            true
        });
        context
    }

    fn scrub_value(&self, value: &mut Value) {
        if let Value::Object(map) = value
            && self.holds_encoded_secret(map)
        {
            *value = Self::encoded_redaction();
            return;
        }
        match value {
            Value::String(text) if self.tokens.occur_in(text) => {
                *text = self.tokens.redact(std::mem::take(text));
            }
            Value::Array(items) => {
                for item in items {
                    self.scrub_value(item);
                }
            }
            Value::Object(map) => map.retain(|key, value| {
                if self.withholds_key(key) {
                    return false;
                }
                self.scrub_value(value);
                true
            }),
            Value::Number(number) if self.tokens.matches_number(number) => {
                *value = Value::String(REDACTED.to_string());
            }
            _ => {}
        }
    }
}
