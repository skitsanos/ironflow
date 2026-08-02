use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use crate::engine::types::Context;

mod owned;

pub(crate) const REDACTED: &str = "[REDACTED]";

/// Redacts invocation-only values before data crosses a durable, event, or
/// public-response boundary.
#[derive(Clone, Debug, Default)]
pub(crate) struct SecretRedactor {
    protected_keys: Arc<HashSet<String>>,
    secrets: Arc<Vec<String>>,
}

impl SecretRedactor {
    pub(crate) fn from_overlay(overlay: &Context) -> Self {
        let protected_keys = overlay.keys().cloned().collect();
        let mut secrets = HashSet::new();
        for value in overlay.values() {
            collect_secret_strings(value, &mut secrets);
        }
        let escaped = secrets
            .iter()
            .filter_map(|secret| serde_json::to_string(secret).ok())
            .filter_map(|encoded| {
                encoded
                    .strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        secrets.extend(escaped);
        let mut secrets = secrets.into_iter().collect::<Vec<_>>();
        secrets.sort_unstable_by_key(|secret| std::cmp::Reverse(secret.len()));

        Self {
            protected_keys: Arc::new(protected_keys),
            secrets: Arc::new(secrets),
        }
    }

    pub(crate) fn redact_context(&self, context: &Context) -> Context {
        context
            .iter()
            .filter(|(key, _)| !self.protected_keys.contains(*key))
            .filter(|(key, _)| self.redact_text(key) == **key)
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
                    .filter(|(key, _)| !self.protected_keys.contains(*key))
                    .filter(|(key, _)| self.redact_text(key) == **key)
                    .map(|(key, value)| (key.clone(), self.redact_value(value)))
                    .collect(),
            ),
            Value::Number(number)
                if self
                    .secrets
                    .iter()
                    .any(|secret| secret == &number.to_string()) =>
            {
                Value::String(REDACTED.to_string())
            }
            other => other.clone(),
        }
    }

    pub(crate) fn redact_text(&self, text: &str) -> String {
        let mut redacted = text.to_string();
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
}

fn collect_secret_strings(value: &Value, secrets: &mut HashSet<String>) {
    match value {
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return;
            }
            secrets.insert(text.to_string());

            if let Ok(structured) = serde_json::from_str::<Value>(text) {
                collect_structured_secrets(&structured, secrets);
            }

            // Signature formats commonly use comma/semicolon-delimited
            // key=value components. Track component values too, so extracting
            // `v1` does not evade the persistence fence.
            for component in text.split([',', ';']) {
                if let Some((_, value)) = component.trim().split_once('=') {
                    let value = value.trim().trim_matches('"').trim();
                    if !value.is_empty() {
                        secrets.insert(value.to_string());
                    }
                }
            }

            if let Some((scheme, credential)) = text.split_once(char::is_whitespace)
                && matches!(scheme.to_ascii_lowercase().as_str(), "bearer" | "basic")
                && !credential.trim().is_empty()
            {
                secrets.insert(credential.trim().to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_secret_strings(item, secrets);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                if !key.is_empty() {
                    secrets.insert(key.clone());
                }
                collect_secret_strings(value, secrets);
            }
        }
        Value::Number(number) => {
            secrets.insert(number.to_string());
        }
        _ => {}
    }
}

fn collect_structured_secrets(value: &Value, secrets: &mut HashSet<String>) {
    match value {
        Value::String(text) => {
            let text = text.trim();
            if !text.is_empty() {
                secrets.insert(text.to_string());
            }
        }
        Value::Number(number) => {
            secrets.insert(number.to_string());
        }
        Value::Array(items) => {
            for item in items {
                collect_structured_secrets(item, secrets);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                if !key.is_empty() {
                    secrets.insert(key.clone());
                }
                collect_structured_secrets(value, secrets);
            }
        }
        Value::Bool(_) | Value::Null => {}
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
