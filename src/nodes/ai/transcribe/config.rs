use anyhow::Result;

use crate::engine::types::Context;
use crate::nodes::ai::embeddings::resolve_param;
use crate::util::node_config::{config_f64_or, get_path};

/// Transcript output format requested from the provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TranscriptFormat {
    Vtt,
    Srt,
    Text,
    Json,
}

impl TranscriptFormat {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "vtt" => Ok(Self::Vtt),
            "srt" => Ok(Self::Srt),
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => anyhow::bail!(
                "transcribe: unsupported format '{}'. Must be 'vtt', 'srt', 'text', or 'json'.",
                other
            ),
        }
    }

    /// Value sent as `response_format`. `json` maps to `verbose_json` so the
    /// object carries segments, duration and detected language.
    pub(super) fn as_api_value(self) -> &'static str {
        match self {
            Self::Vtt => "vtt",
            Self::Srt => "srt",
            Self::Text => "text",
            Self::Json => "verbose_json",
        }
    }

    /// Value reported on `<output_key>_format`, echoing what the caller asked for.
    pub(super) fn as_label(self) -> &'static str {
        match self {
            Self::Vtt => "vtt",
            Self::Srt => "srt",
            Self::Text => "text",
            Self::Json => "json",
        }
    }

    pub(super) fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Provider {
    OpenAi,
    OpenAiCompatible,
    Azure,
}

impl Provider {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "openai" => Ok(Self::OpenAi),
            "openai_compatible" => Ok(Self::OpenAiCompatible),
            "azure" => Ok(Self::Azure),
            other => anyhow::bail!(
                "transcribe: unsupported provider '{}'. Must be 'openai', 'openai_compatible', or 'azure'.",
                other
            ),
        }
    }
}

#[derive(Debug)]
pub(super) struct TranscribeConfig {
    pub(super) path: String,
    pub(super) provider: Provider,
    pub(super) model: String,
    pub(super) api_key: String,
    pub(super) base_url: String,
    pub(super) api_version: Option<String>,
    pub(super) format: TranscriptFormat,
    pub(super) language: Option<String>,
    pub(super) prompt: Option<String>,
    pub(super) temperature: Option<f64>,
    pub(super) output_key: String,
    pub(super) output_file: Option<String>,
    pub(super) timeout_s: f64,
}

pub(super) fn resolve(config: &serde_json::Value, ctx: &Context) -> Result<TranscribeConfig> {
    let path = get_path(config, ctx, "transcribe")?;

    let provider = Provider::parse(
        config
            .get("provider")
            .and_then(|value| value.as_str())
            .unwrap_or("openai"),
    )?;

    let format = TranscriptFormat::parse(
        config
            .get("format")
            .and_then(|value| value.as_str())
            .unwrap_or("vtt"),
    )?;

    let (key_env, url_env, default_url) = match provider {
        Provider::Azure => ("AZURE_OPENAI_API_KEY", "AZURE_OPENAI_ENDPOINT", ""),
        _ => (
            "OPENAI_API_KEY",
            "OPENAI_BASE_URL",
            "https://api.openai.com/v1",
        ),
    };

    let api_key = resolve_param(config, "api_key", key_env, ctx).ok_or_else(|| {
        anyhow::anyhow!(
            "transcribe requires 'api_key' or the {} environment variable",
            key_env
        )
    })?;

    let base_url = resolve_param(config, "base_url", url_env, ctx)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default_url.to_string());

    if base_url.trim().is_empty() {
        anyhow::bail!(
            "transcribe requires 'base_url' or the {} environment variable for this provider",
            url_env
        );
    }

    let api_version = match provider {
        Provider::Azure => Some(
            resolve_param(config, "api_version", "AZURE_OPENAI_API_VERSION", ctx).ok_or_else(
                || {
                    anyhow::anyhow!(
                        "transcribe with provider 'azure' requires 'api_version' or AZURE_OPENAI_API_VERSION"
                    )
                },
            )?,
        ),
        _ => None,
    };

    // `model` and `language` are plain config values with context
    // interpolation. They deliberately have no environment fallback: the spec
    // defines environment variables only for credentials and endpoints, and
    // inventing undocumented ones would widen the public contract.
    let interpolated = |key: &str| -> Option<String> {
        config
            .get(key)
            .and_then(|value| value.as_str())
            .map(|value| crate::lua::interpolate::interpolate_ctx(value, ctx))
    };

    Ok(TranscribeConfig {
        path,
        provider,
        model: interpolated("model").unwrap_or_else(|| "whisper-1".to_string()),
        api_key,
        base_url,
        api_version,
        format,
        language: interpolated("language"),
        prompt: interpolated("prompt"),
        temperature: config.get("temperature").and_then(|value| value.as_f64()),
        output_key: config
            .get("output_key")
            .and_then(|value| value.as_str())
            .unwrap_or("transcript")
            .to_string(),
        output_file: interpolated("output_file"),
        timeout_s: config_f64_or(config, "timeout", ctx, 120.0)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_parses_supported_values_and_rejects_others() {
        assert_eq!(
            TranscriptFormat::parse("vtt").unwrap(),
            TranscriptFormat::Vtt
        );
        assert_eq!(
            TranscriptFormat::parse("srt").unwrap(),
            TranscriptFormat::Srt
        );
        assert_eq!(
            TranscriptFormat::parse("text").unwrap(),
            TranscriptFormat::Text
        );
        assert_eq!(
            TranscriptFormat::parse("json").unwrap(),
            TranscriptFormat::Json
        );

        let error = TranscriptFormat::parse("mp3").unwrap_err().to_string();
        assert!(error.contains("unsupported format"), "{error}");
    }

    #[test]
    fn json_format_requests_verbose_payload() {
        assert_eq!(TranscriptFormat::Json.as_api_value(), "verbose_json");
        assert_eq!(TranscriptFormat::Vtt.as_api_value(), "vtt");
        assert!(TranscriptFormat::Json.is_json());
        assert!(!TranscriptFormat::Vtt.is_json());
    }

    #[test]
    fn provider_defaults_to_openai_and_rejects_unknown() {
        assert_eq!(Provider::parse("openai").unwrap(), Provider::OpenAi);
        assert_eq!(
            Provider::parse("openai_compatible").unwrap(),
            Provider::OpenAiCompatible
        );
        assert_eq!(Provider::parse("azure").unwrap(), Provider::Azure);

        let error = Provider::parse("deepgram").unwrap_err().to_string();
        assert!(error.contains("unsupported provider"), "{error}");
    }

    #[test]
    fn missing_credential_names_the_parameter_and_the_environment_variable() {
        // Guard the env var so the assertion holds on a developer machine that
        // exports a real key.
        let saved = std::env::var("OPENAI_API_KEY").ok();
        unsafe { std::env::remove_var("OPENAI_API_KEY") };

        let config = serde_json::json!({ "path": "/tmp/a.mp3" });
        let error = resolve(&config, &Context::new()).unwrap_err().to_string();

        if let Some(value) = saved {
            unsafe { std::env::set_var("OPENAI_API_KEY", value) };
        }

        assert!(error.contains("api_key"), "{error}");
        assert!(error.contains("OPENAI_API_KEY"), "{error}");
    }
}
