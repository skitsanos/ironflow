//! Response header fields as context values.

use base64::{Engine as _, engine::general_purpose::STANDARD};

/// A value is text when its bytes are UTF-8 and hold no control character
/// other than horizontal tab (IF-264). The client already fails a response
/// whose header holds DEL or a C0 control other than tab, so in practice only
/// invalid UTF-8 and C1 controls keep the `{"base64": ...}` byte form (IF-258).
fn response_header_value(value: &reqwest::header::HeaderValue) -> serde_json::Value {
    header_bytes_value(value.as_bytes())
}

fn header_bytes_value(bytes: &[u8]) -> serde_json::Value {
    match std::str::from_utf8(bytes) {
        Ok(text) if !text.chars().any(|c| c.is_control() && c != '\t') => {
            serde_json::Value::from(text)
        }
        _ => serde_json::json!({ "base64": STANDARD.encode(bytes) }),
    }
}

pub(super) fn response_headers(
    headers: &reqwest::header::HeaderMap,
) -> serde_json::Map<String, serde_json::Value> {
    headers
        .keys()
        .filter_map(|name| {
            let mut values = headers.get_all(name).iter().map(response_header_value);
            let first = values.next()?;
            let field = match values.next() {
                None => first,
                Some(second) => {
                    serde_json::Value::Array([first, second].into_iter().chain(values).collect())
                }
            };
            Some((name.to_string(), field))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_text_is_utf8_without_controls_other_than_tab() {
        let text = |bytes: &[u8]| header_bytes_value(bytes);
        let encoded = |bytes: &[u8]| serde_json::json!({ "base64": STANDARD.encode(bytes) });
        assert_eq!(text(b""), "");
        assert_eq!(text("résumé.pdf".as_bytes()), "résumé.pdf");
        assert_eq!(text(b"a\tb"), "a\tb");
        for bytes in [&b"caf\xe9"[..], b"a\x7fb", "a\u{85}b".as_bytes(), b"a\x01b"] {
            assert_eq!(text(bytes), encoded(bytes), "{bytes:?}");
        }
    }
}
