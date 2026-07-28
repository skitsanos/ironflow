use anyhow::Result;

use crate::util::sensitive_url::redact_sensitive_text;

use super::config::TranscriptFormat;

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

        anyhow::bail!(
            "transcribe: provider returned {}: {}",
            status.as_u16(),
            redact_sensitive_text(&detail)
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
}
