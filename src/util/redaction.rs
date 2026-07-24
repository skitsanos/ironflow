use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use crate::engine::types::Context;

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
mod tests {
    use super::*;

    #[test]
    fn overlay_redactor_removes_keys_and_nested_secret_values() {
        let overlay = Context::from([(
            "_headers".to_string(),
            serde_json::json!({"stripe-signature": "t=12345678,v1=super-secret-value"}),
        )]);
        let redactor = SecretRedactor::from_overlay(&overlay);
        let context = Context::from([
            ("_headers".to_string(), overlay["_headers"].clone()),
            (
                "result".to_string(),
                serde_json::json!({
                    "copy": "t=12345678,v1=super-secret-value",
                    "component": "super-secret-value",
                    "message": "signature: t=12345678,v1=super-secret-value",
                    "super-secret-value": true
                }),
            ),
        ]);

        let redacted = redactor.redact_context(&context);

        assert!(!redacted.contains_key("_headers"));
        let serialized = serde_json::to_string(&redacted).unwrap();
        assert!(!serialized.contains("super-secret-value"));
        assert!(serialized.contains(REDACTED));
    }

    #[test]
    fn public_redaction_uses_legacy_headers_to_scrub_the_whole_record() {
        let mut value = serde_json::json!({
            "ctx": {
                "_headers": {"authorization": "Bearer old-secret-token"},
                "auth_token": "old-secret-token"
            },
            "tasks": {"check": {"error": "failed with old-secret-token"}},
            "safe": "visible"
        });

        redact_legacy_webhook_record(&mut value);

        assert!(value["ctx"].get("_headers").is_none());
        assert_eq!(value["ctx"]["auth_token"], REDACTED);
        assert!(
            !value["tasks"]["check"]["error"]
                .as_str()
                .unwrap()
                .contains("old-secret-token")
        );
        assert_eq!(value["safe"], "visible");
    }

    #[test]
    fn legacy_ordinary_short_headers_do_not_corrupt_run_fields() {
        let mut value = serde_json::json!({
            "id": "run-2026",
            "ctx": {
                "_headers": {"content-length": "2", "authorization": "Bearer old-secret-token"},
                "copy": "old-secret-token"
            }
        });

        redact_legacy_webhook_record(&mut value);

        assert_eq!(value["id"], "run-2026");
        assert_eq!(value["ctx"]["copy"], REDACTED);
    }

    #[test]
    fn structured_and_numeric_secret_forms_are_redacted() {
        let overlay = Context::from([(
            "_headers".to_string(),
            serde_json::json!({
                "x-signature": "{\"token\":\"long-secret-123\",\"code\":12345678}"
            }),
        )]);
        let redactor = SecretRedactor::from_overlay(&overlay);
        let context = Context::from([
            (
                "result".to_string(),
                Value::String("long-secret-123".to_string()),
            ),
            ("numeric_result".to_string(), serde_json::json!(12345678)),
            ("long-secret-123".to_string(), Value::Bool(true)),
        ]);

        let redacted = redactor.redact_context(&context);

        assert_eq!(redacted["result"], REDACTED);
        assert_eq!(redacted["numeric_result"], REDACTED);
        assert!(!redacted.contains_key("long-secret-123"));
    }

    #[test]
    fn public_redaction_leaves_non_webhook_records_untouched() {
        let mut value = serde_json::json!({"ctx": {"signature": "business-value"}});

        redact_legacy_webhook_record(&mut value);

        assert_eq!(value["ctx"]["signature"], "business-value");
    }

    #[test]
    fn public_redaction_handles_run_arrays() {
        let mut value = serde_json::json!([
            {"ctx": {"_headers": {"x-signature": "first-secret"}, "copy": "first-secret"}},
            {"ctx": {"safe": "visible"}}
        ]);

        redact_legacy_webhook_record(&mut value);

        assert!(value[0]["ctx"].get("_headers").is_none());
        assert_eq!(value[0]["ctx"]["copy"], REDACTED);
        assert_eq!(value[1]["ctx"]["safe"], "visible");
    }
}
