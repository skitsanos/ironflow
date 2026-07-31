use anyhow::Result;
use reqwest::multipart::{Form, Part};

use crate::util::sensitive_url::redact_sensitive_text;

use super::config::{Provider, TranscribeConfig};

const MAX_SAME_ORIGIN_REDIRECTS: usize = 10;

/// Permit provider redirects only while they remain on the original origin.
///
/// A transcription request carries both a credential and the caller's audio.
/// Reqwest strips `Authorization` in some redirect cases, but it does not
/// provide equivalent protection for Azure's custom `api-key` header or for
/// the multipart body. Refusing an origin change is therefore the only safe
/// default. Same-origin redirects remain useful for provider migrations and
/// canonical endpoint paths, with an explicit hop ceiling to bound loops.
pub(super) fn same_origin_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > MAX_SAME_ORIGIN_REDIRECTS {
            return attempt.error("too many transcribe redirects");
        }

        let Some(origin) = attempt.previous().first() else {
            return attempt.error("transcribe redirect has no source origin");
        };
        let target = attempt.url();
        if origin.scheme() != target.scheme()
            || origin.host() != target.host()
            || origin.port_or_known_default() != target.port_or_known_default()
        {
            attempt.error("cross-origin transcribe redirect refused")
        } else {
            attempt.follow()
        }
    })
}

/// Build the transcription endpoint. Azure addresses a deployment in the path
/// and requires an api-version query parameter; the OpenAI-shaped providers
/// append a fixed suffix to their base URL.
pub(super) fn endpoint(config: &TranscribeConfig) -> String {
    let base = config.base_url.trim_end_matches('/');
    match config.provider {
        Provider::Azure => format!(
            "{}/openai/deployments/{}/audio/transcriptions?api-version={}",
            base,
            config.model,
            config.api_version.as_deref().unwrap_or_default()
        ),
        _ => format!("{}/audio/transcriptions", base),
    }
}

/// Upload the audio and return the raw status and body. Interpreting them is
/// `response::interpret`'s job.
pub(super) async fn send(
    client: &reqwest::Client,
    config: &TranscribeConfig,
    audio: Vec<u8>,
    file_name: &str,
    max_response_bytes: u64,
) -> Result<(reqwest::StatusCode, String)> {
    let mut form = Form::new()
        .part("file", Part::bytes(audio).file_name(file_name.to_string()))
        .text("response_format", config.format.as_api_value());

    // Azure selects the model by deployment in the URL, so the field is
    // redundant there and some deployments reject it.
    if config.provider != Provider::Azure {
        form = form.text("model", config.model.clone());
    }
    if let Some(language) = &config.language {
        form = form.text("language", language.clone());
    }
    if let Some(prompt) = &config.prompt {
        form = form.text("prompt", prompt.clone());
    }
    if let Some(temperature) = config.temperature {
        form = form.text("temperature", temperature.to_string());
    }

    let url = endpoint(config);
    let request = match config.provider {
        Provider::Azure => client.post(&url).header("api-key", &config.api_key),
        _ => client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key)),
    };

    let response = request.multipart(form).send().await.map_err(|error| {
        anyhow::anyhow!(
            "transcribe request failed: {}",
            redact_sensitive_text(&error.to_string())
        )
    })?;

    let status = response.status();
    let body = read_capped_response(response, max_response_bytes).await?;

    Ok((status, body))
}

/// Read a provider body while retaining no more than `max_bytes + 1` bytes.
///
/// `Content-Length` is only an optimization: arbitrary compatible providers
/// may omit it or send a chunked body, so the authoritative check runs for
/// every chunk. On the crossing chunk we copy at most the one extra byte
/// needed to prove overflow, then fail before error parsing or `output_file`
/// handling can observe the oversized body.
async fn read_capped_response(mut response: reqwest::Response, max_bytes: u64) -> Result<String> {
    if let Some(content_length) = response.content_length()
        && content_length > max_bytes
    {
        anyhow::bail!(
            "transcribe: provider response content-length {content_length} exceeds \
             IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES ({max_bytes})"
        );
    }

    let capacity = response
        .content_length()
        .unwrap_or(0)
        .min(max_bytes)
        .min(64 * 1024)
        .try_into()
        .unwrap_or(0);
    let mut body = Vec::with_capacity(capacity);

    while let Some(chunk) = response.chunk().await.map_err(|error| {
        anyhow::anyhow!(
            "transcribe: failed to read provider response: {}",
            redact_sensitive_text(&error.to_string())
        )
    })? {
        let current = body.len() as u64;
        let remaining = max_bytes.saturating_sub(current);
        if chunk.len() as u64 > remaining {
            let proof_len = remaining
                .saturating_add(1)
                .min(chunk.len() as u64)
                .try_into()
                .unwrap_or(chunk.len());
            body.extend_from_slice(&chunk[..proof_len]);
            anyhow::bail!(
                "transcribe: provider response exceeded \
                 IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES ({max_bytes}) while streaming"
            );
        }
        body.extend_from_slice(&chunk);
    }

    String::from_utf8(body).map_err(|error| {
        anyhow::anyhow!(
            "transcribe: provider response is not valid UTF-8: {}",
            redact_sensitive_text(&error.to_string())
        )
    })
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use axum::Router;
    use axum::body::{Body, Bytes};
    use axum::routing::get;

    use super::super::config::{Provider, TranscriptFormat};
    use super::*;

    fn config_for(provider: Provider, base_url: &str) -> TranscribeConfig {
        TranscribeConfig {
            path: "/tmp/a.mp3".to_string(),
            provider,
            model: "whisper-1".to_string(),
            api_key: "test-key".to_string(),
            base_url: base_url.to_string(),
            api_version: Some("2024-06-01".to_string()),
            format: TranscriptFormat::Vtt,
            language: None,
            prompt: None,
            temperature: None,
            output_key: "transcript".to_string(),
            output_file: None,
            timeout_s: 120.0,
        }
    }

    #[test]
    fn openai_endpoint_appends_audio_transcriptions() {
        let config = config_for(Provider::OpenAi, "https://api.openai.com/v1");
        assert_eq!(
            endpoint(&config),
            "https://api.openai.com/v1/audio/transcriptions"
        );
    }

    #[test]
    fn openai_endpoint_tolerates_a_trailing_slash() {
        let config = config_for(
            Provider::OpenAiCompatible,
            "https://api.groq.com/openai/v1/",
        );
        assert_eq!(
            endpoint(&config),
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
    }

    #[test]
    fn azure_endpoint_uses_the_deployment_path_and_api_version() {
        let config = config_for(Provider::Azure, "https://example.openai.azure.com");
        assert_eq!(
            endpoint(&config),
            "https://example.openai.azure.com/openai/deployments/whisper-1/audio/transcriptions?api-version=2024-06-01"
        );
    }

    async fn chunked_response(parts: &'static [&'static [u8]]) -> reqwest::Response {
        let app = Router::new().route(
            "/",
            get(move || async move {
                let chunks = futures_util::stream::iter(
                    parts
                        .iter()
                        .map(|part| Ok::<_, Infallible>(Bytes::from_static(part))),
                );
                Body::from_stream(chunks)
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        reqwest::get(format!("http://{address}/")).await.unwrap()
    }

    #[tokio::test]
    async fn chunked_response_at_the_limit_is_accepted_without_content_length() {
        let response = chunked_response(&[b"four", b"more"]).await;
        assert_eq!(response.content_length(), None);

        let body = read_capped_response(response, 8).await.unwrap();
        assert_eq!(body, "fourmore");
    }

    #[tokio::test]
    async fn max_plus_one_chunked_byte_is_rejected_before_interpretation() {
        let response = chunked_response(&[b"four", b"more", b"!"]).await;
        assert_eq!(response.content_length(), None);

        let error = read_capped_response(response, 8)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES (8)"),
            "{error}"
        );
        assert!(error.contains("while streaming"), "{error}");
    }
}
