//! Secrets inside values IronFlow itself encoded.
//!
//! The HTTP nodes report a response header that is not text as
//! `{"base64": "<bytes>"}` (IF-258). Text matching cannot see a secret in
//! that form, so an upstream that echoes a protected value in such a header
//! would have it stored as it is. The scrubber therefore decodes an object of
//! exactly that shape and replaces it when the bytes hold a secret (IF-262).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value};

use super::{REDACTED, SecretRedactor};

const KEY: &str = "base64";

impl SecretRedactor {
    /// Whether `map` is exactly `{"base64": "<standard Base64>"}` and its
    /// decoded bytes contain a protected value. Bytes that are not UTF-8 are
    /// read lossily: a secret is text, so its own bytes survive that.
    pub(super) fn holds_encoded_secret(&self, map: &Map<String, Value>) -> bool {
        if map.len() != 1 || self.is_empty() {
            return false;
        }
        let Some(Value::String(encoded)) = map.get(KEY) else {
            return false;
        };
        STANDARD
            .decode(encoded)
            .is_ok_and(|bytes| self.tokens.occur_in(&String::from_utf8_lossy(&bytes)))
    }

    /// The same shape with the marker in place of the bytes, so a reader of
    /// the record still finds an object where it expects one.
    pub(super) fn encoded_redaction() -> Value {
        serde_json::json!({ KEY: STANDARD.encode(REDACTED) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::Context;
    use serde_json::json;

    const SECRET: &str = "s3cr3t-token-value";

    fn redactor() -> SecretRedactor {
        let mut overlay = Context::new();
        overlay.insert("_headers".to_string(), json!({ "x-api-key": SECRET }));
        SecretRedactor::from_overlay(&overlay)
    }

    fn encoded(bytes: &[u8]) -> Value {
        json!({ KEY: STANDARD.encode(bytes) })
    }

    #[test]
    fn a_secret_inside_encoded_bytes_is_replaced_in_both_scrubbers() {
        let mut bytes = b"caf\xe9 ".to_vec();
        bytes.extend_from_slice(SECRET.as_bytes());
        let headers = json!({ "x-echo": encoded(&bytes), "x-other": ["plain", encoded(&bytes)] });
        let expected = json!({
            "x-echo": SecretRedactor::encoded_redaction(),
            "x-other": ["plain", SecretRedactor::encoded_redaction()],
        });

        assert_eq!(redactor().redact_value(&headers), expected);

        let mut context = Context::new();
        context.insert("response_headers".to_string(), headers);
        let scrubbed = redactor().redact_context_owned(context);
        assert_eq!(scrubbed["response_headers"], expected);
        assert_eq!(
            STANDARD
                .decode(expected["x-echo"][KEY].as_str().unwrap())
                .unwrap(),
            REDACTED.as_bytes()
        );
    }

    #[test]
    fn a_secret_inside_a_utf8_header_string_is_replaced_as_text() {
        // Since IF-264 a UTF-8 header value reaches the record as a string,
        // not as Base64, so the ordinary text scan must find the secret.
        let value = format!("résumé {SECRET} naïve");
        let headers = json!({ "x-echo": value, "x-other": ["plain", value] });
        let expected_text = format!("résumé {REDACTED} naïve");
        let expected = json!({
            "x-echo": expected_text,
            "x-other": ["plain", expected_text],
        });

        assert_eq!(redactor().redact_value(&headers), expected);

        let mut context = Context::new();
        context.insert("response_headers".to_string(), headers);
        let scrubbed = redactor().redact_context_owned(context);
        assert_eq!(scrubbed["response_headers"], expected);
    }

    #[test]
    fn other_encoded_values_and_other_shapes_are_left_alone() {
        let harmless = encoded(b"caf\xe9");
        assert_eq!(redactor().redact_value(&harmless), harmless);

        // Not this shape: a second key, a value that is not Base64, and a
        // string that only looks like the field.
        let mut bytes = SECRET.as_bytes().to_vec();
        bytes.push(0xe9);
        let two_keys = json!({ KEY: STANDARD.encode(&bytes), "name": "x" });
        assert_eq!(redactor().redact_value(&two_keys), two_keys);
        let not_base64 = json!({ KEY: "not base64 at all!" });
        assert_eq!(redactor().redact_value(&not_base64), not_base64);

        // No overlay: nothing is decoded.
        let plain = SecretRedactor::default();
        let value = encoded(&bytes);
        assert_eq!(plain.redact_value(&value), value);
    }
}
