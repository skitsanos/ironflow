use anyhow::Result;
use reqwest::multipart::{Form, Part};

use crate::util::sensitive_url::redact_sensitive_text;

use super::config::{Provider, TranscribeConfig};

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
    let body = response.text().await.map_err(|error| {
        anyhow::anyhow!(
            "transcribe: failed to read provider response: {}",
            redact_sensitive_text(&error.to_string())
        )
    })?;

    Ok((status, body))
}

#[cfg(test)]
mod tests {
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
}
