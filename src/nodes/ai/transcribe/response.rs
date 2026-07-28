use anyhow::Result;

use crate::util::sensitive_url::redact_sensitive_text;

use super::config::TranscriptFormat;

/// Maximum number of **characters** (not bytes) of a provider's error body
/// embedded in this node's error message.
///
/// `base_url` is caller-supplied and arbitrary -- it can address any HTTP
/// server, not only a well-behaved Whisper-style API -- so a large or
/// hostile response body must never be embedded verbatim: this message is
/// persisted into run state as `_error_message`, and an unbounded provider
/// response would bloat stored state without limit. A couple thousand
/// characters is generous for the realistic case (providers return a short
/// JSON `message` field) while keeping worst-case storage bounded
/// regardless of how large the underlying HTTP response actually was.
const MAX_ERROR_DETAIL_CHARS: usize = 2_000;

/// Appended when `detail` is cut short, so a reader can tell the message was
/// truncated rather than the provider being unusually terse.
const TRUNCATION_MARKER: &str = "… [truncated]";

/// Bound `text` to at most `max_chars` **characters**.
///
/// Slicing a `&str` by byte index can land inside a multi-byte UTF-8 code
/// point and panic; counting and collecting `chars()` instead always cuts on
/// a character boundary.
fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{head}{TRUNCATION_MARKER}")
    } else {
        head
    }
}

/// Turn a provider status and body into the transcript value, or an error that
/// names the failure without disclosing credentials.
pub(super) fn interpret(
    status: reqwest::StatusCode,
    body: &str,
    format: TranscriptFormat,
) -> Result<serde_json::Value> {
    if !status.is_success() {
        let detail = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(|message| message.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| body.trim().to_string());

        // Redact first, while `detail` is still complete: pattern-based
        // redaction needs an intact credential/DSN shape to recognise, and
        // truncating beforehand could bisect it. Truncate second, so the
        // bound applies to what actually reaches the error string.
        let detail = truncate_chars(&redact_sensitive_text(&detail), MAX_ERROR_DETAIL_CHARS);

        anyhow::bail!(
            "transcribe: provider returned {}: {}",
            status.as_u16(),
            detail
        );
    }

    if body.trim().is_empty() {
        anyhow::bail!("transcribe: provider returned an empty transcript");
    }

    if format.is_json() {
        let value: serde_json::Value = serde_json::from_str(body).map_err(|error| {
            anyhow::anyhow!(
                "transcribe: provider response could not be parsed as JSON: {}",
                error
            )
        })?;
        return Ok(value);
    }

    Ok(serde_json::Value::String(body.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VTT: &str = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nHello.\n";

    #[test]
    fn success_returns_the_body_verbatim_for_text_formats() {
        let value = interpret(reqwest::StatusCode::OK, VTT, TranscriptFormat::Vtt).unwrap();
        assert_eq!(value.as_str().unwrap(), VTT);
    }

    #[test]
    fn success_parses_the_body_for_json_format() {
        let body = r#"{"text":"Hello.","duration":2.0}"#;
        let value = interpret(reqwest::StatusCode::OK, body, TranscriptFormat::Json).unwrap();
        assert_eq!(value.get("text").unwrap().as_str().unwrap(), "Hello.");
    }

    #[test]
    fn malformed_json_body_fails_explicitly() {
        let error = interpret(reqwest::StatusCode::OK, "not json", TranscriptFormat::Json)
            .unwrap_err()
            .to_string();
        assert!(error.contains("could not be parsed"), "{error}");
    }

    #[test]
    fn provider_error_surfaces_status_and_message() {
        let body = r#"{"error":{"message":"Invalid API key"}}"#;
        let error = interpret(
            reqwest::StatusCode::UNAUTHORIZED,
            body,
            TranscriptFormat::Vtt,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("401"), "{error}");
        assert!(error.contains("Invalid API key"), "{error}");
    }

    #[test]
    fn empty_success_body_is_rejected() {
        let error = interpret(reqwest::StatusCode::OK, "   ", TranscriptFormat::Vtt)
            .unwrap_err()
            .to_string();
        assert!(error.contains("empty transcript"), "{error}");
    }

    #[test]
    fn oversized_provider_error_body_is_truncated_with_marker() {
        let huge_message = "e".repeat(10_000);
        let body = format!(r#"{{"error":{{"message":"{huge_message}"}}}}"#);
        let error = interpret(
            reqwest::StatusCode::BAD_REQUEST,
            &body,
            TranscriptFormat::Vtt,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains(TRUNCATION_MARKER), "{error}");
        // Bounded well below the original 10,000-character message: the
        // "transcribe: provider returned 400: " prefix plus at most
        // MAX_ERROR_DETAIL_CHARS plus the marker, not the full body.
        assert!(error.chars().count() < 2_100, "{error}");
    }

    #[test]
    fn truncation_cuts_on_a_char_boundary_not_a_byte_index() {
        // Every character here is a multi-byte UTF-8 code point. If
        // truncation ever sliced by byte index instead of character count,
        // building this string would panic on a non-boundary byte.
        let huge_message = "é".repeat(3_000);
        let body = format!(r#"{{"error":{{"message":"{huge_message}"}}}}"#);
        let error = interpret(
            reqwest::StatusCode::BAD_REQUEST,
            &body,
            TranscriptFormat::Vtt,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains(TRUNCATION_MARKER), "{error}");
    }
}
