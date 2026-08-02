use super::*;

fn base_config(temperature: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "path": "/tmp/a.mp3",
        "api_key": "test-key",
        "temperature": temperature,
    })
}

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

// Credential-environment tests live in `tests/test_transcribe_config_env.rs`
// because mutating process-global environment values in this shared unit-test
// binary would race other modules.

#[test]
fn temperature_resolves_numeric_context_interpolation() {
    let ctx: Context = [("temp".to_string(), serde_json::json!(0.25))]
        .into_iter()
        .collect();
    let config = base_config(serde_json::json!("${ctx.temp}"));

    let resolved = resolve(&config, &ctx).unwrap();
    assert_eq!(resolved.temperature, Some(0.25));
}

#[test]
fn invalid_and_nonfinite_temperature_values_are_rejected_not_dropped() {
    for value in [serde_json::json!("warm"), serde_json::json!("NaN")] {
        let error = resolve(&base_config(value), &Context::new())
            .unwrap_err()
            .to_string();
        assert!(error.contains("temperature"), "{error}");
        assert!(error.contains("finite number"), "{error}");
    }

    let ctx: Context = [("temp".to_string(), serde_json::json!("inf"))]
        .into_iter()
        .collect();
    let error = resolve(&base_config(serde_json::json!("${ctx.temp}")), &ctx)
        .unwrap_err()
        .to_string();
    assert!(error.contains("finite number"), "{error}");
}

#[test]
fn temperature_enforces_the_provider_range_inclusive() {
    for valid in [0.0, 0.5, 1.0] {
        assert_eq!(
            resolve(&base_config(serde_json::json!(valid)), &Context::new())
                .unwrap()
                .temperature,
            Some(valid)
        );
    }
    for invalid in [-0.01, 1.01] {
        let error = resolve(&base_config(serde_json::json!(invalid)), &Context::new())
            .unwrap_err()
            .to_string();
        assert!(error.contains("between 0 and 1 inclusive"), "{error}");
    }
}
