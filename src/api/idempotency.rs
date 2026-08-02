//! Durable request identity without storing the caller's raw key.

use axum::http::HeaderMap;
use sha2::{Digest as _, Sha256};

use crate::engine::types::Context;

use super::errors::AppError;

pub(crate) const CONTEXT_KEY: &str = "_ironflow_idempotency";
const HEADER: &str = "idempotency-key";
const MAX_KEY_BYTES: usize = 128;

pub(crate) struct RequestIdentity {
    pub(crate) run_id: String,
    pub(crate) fingerprint: String,
}

impl RequestIdentity {
    pub(crate) fn from_request(
        headers: &HeaderMap,
        source: Option<&str>,
        source_base64: Option<&str>,
        file: Option<&str>,
        context: Option<&Context>,
    ) -> Result<Option<Self>, AppError> {
        let Some(raw) = headers.get(HEADER) else {
            return Ok(None);
        };
        let key = raw.to_str().map_err(|_| invalid_key())?;
        validate_key(key)?;
        if context.is_some_and(|context| context.contains_key(CONTEXT_KEY)) {
            return Err(AppError::BadRequest(format!(
                "request context must not define reserved key '{CONTEXT_KEY}'"
            )));
        }

        let run_id = format!("idem-{}", digest_bytes(key.as_bytes()));
        let mut request = serde_json::Map::new();
        let (kind, value) = match (source, source_base64, file) {
            (Some(value), None, None) => ("source", value),
            (None, Some(value), None) => ("source_base64", value),
            (None, None, Some(value)) => ("file", value),
            _ => {
                return Ok(Some(Self {
                    run_id,
                    fingerprint: String::new(),
                }));
            }
        };
        request.insert("kind".to_string(), kind.into());
        request.insert("value".to_string(), value.into());
        request.insert(
            "context".to_string(),
            context
                .cloned()
                .map(serde_json::to_value)
                .transpose()
                .map_err(anyhow::Error::from)
                .map_err(AppError::Internal)?
                .unwrap_or(serde_json::Value::Null),
        );
        let fingerprint = digest_value(&serde_json::Value::Object(request));
        Ok(Some(Self {
            run_id,
            fingerprint,
        }))
    }

    pub(crate) fn insert_marker(&self, context: &mut Context) {
        context.insert(CONTEXT_KEY.to_string(), self.fingerprint.clone().into());
    }

    pub(crate) fn matches(&self, context: &Context) -> bool {
        context.get(CONTEXT_KEY).and_then(serde_json::Value::as_str)
            == Some(self.fingerprint.as_str())
    }
}

fn validate_key(key: &str) -> Result<(), AppError> {
    if key.is_empty()
        || key.len() > MAX_KEY_BYTES
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(invalid_key());
    }
    Ok(())
}

fn invalid_key() -> AppError {
    AppError::BadRequest(format!(
        "Idempotency-Key must contain 1 through {MAX_KEY_BYTES} ASCII letters, digits, '-', '_', '.', or ':'"
    ))
}

fn digest_value(value: &serde_json::Value) -> String {
    let mut hasher = Sha256::new();
    hash_value(value, &mut hasher);
    hex(&hasher.finalize())
}

fn digest_bytes(value: &[u8]) -> String {
    hex(&Sha256::digest(value))
}

fn hash_value(value: &serde_json::Value, hasher: &mut Sha256) {
    match value {
        serde_json::Value::Null => hasher.update(b"n"),
        serde_json::Value::Bool(value) => hasher.update(if *value { b"t" } else { b"f" }),
        serde_json::Value::Number(value) => hash_part(b'#', value.to_string().as_bytes(), hasher),
        serde_json::Value::String(value) => hash_part(b's', value.as_bytes(), hasher),
        serde_json::Value::Array(values) => {
            hasher.update(b"[");
            for value in values {
                hash_value(value, hasher);
            }
            hasher.update(b"]");
        }
        serde_json::Value::Object(values) => {
            hasher.update(b"{");
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            for key in keys {
                hash_part(b'k', key.as_bytes(), hasher);
                hash_value(&values[key], hasher);
            }
            hasher.update(b"}");
        }
    }
}

fn hash_part(tag: u8, value: &[u8], hasher: &mut Sha256) {
    hasher.update([tag]);
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_ignore_object_insertion_order() {
        let left = serde_json::json!({"a": 1, "b": {"x": true, "y": false}});
        let right = serde_json::json!({"b": {"y": false, "x": true}, "a": 1});
        assert_eq!(digest_value(&left), digest_value(&right));
    }

    #[test]
    fn raw_key_is_not_present_in_run_identity() {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER, "customer-secret-key".parse().unwrap());
        let identity =
            RequestIdentity::from_request(&headers, Some("return flow"), None, None, None)
                .unwrap()
                .unwrap();
        assert!(!identity.run_id.contains("customer-secret-key"));
        assert!(identity.run_id.starts_with("idem-"));
    }
}
