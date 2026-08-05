use axum::http::HeaderValue;

use super::*;

#[test]
fn explicit_headers_are_normalized_and_deduplicated() {
    let config = WebhookConfig::new(
        "signed.lua",
        [
            "Stripe-Signature".to_string(),
            "stripe-signature".to_string(),
        ],
    )
    .unwrap();

    assert_eq!(
        config.forward_headers().collect::<Vec<_>>(),
        ["stripe-signature"]
    );
}

#[test]
fn platform_credentials_cannot_be_forwarded() {
    for header in RESERVED_CREDENTIAL_HEADERS {
        let error = WebhookConfig::new("flow.lua", [header.to_string()]).unwrap_err();
        assert!(
            error.contains("reserved"),
            "unexpected error for {header}: {error}"
        );
    }
}

#[test]
fn overlay_contains_only_configured_headers() {
    let config = WebhookConfig::new("signed.lua", ["stripe-signature".to_string()]).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("stripe-signature", HeaderValue::from_static("v1=secret"));
    headers.insert("authorization", HeaderValue::from_static("Bearer platform"));

    let overlay = config.execution_overlay(&headers).unwrap();

    assert_eq!(
        overlay[EXECUTION_HEADERS_KEY],
        serde_json::json!({"stripe-signature": "v1=secret"})
    );
}

#[test]
fn short_confidential_values_are_rejected_before_redaction() {
    let config = WebhookConfig::new("signed.lua", ["x-signature".to_string()]).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("x-signature", HeaderValue::from_static("a"));

    let error = config.execution_overlay(&headers).unwrap_err();

    assert!(error.contains("at least 8"));

    headers.insert("x-signature", HeaderValue::from_static("a      b"));
    let error = config.execution_overlay(&headers).unwrap_err();
    assert!(error.contains("at least 8"));
}

#[test]
fn signature_header_cannot_be_forwarded_or_use_platform_credentials() {
    let signature =
        WebhookSignatureConfig::hmac_sha256("x-signature", "WEBHOOK_SECRET", "sha256=").unwrap();
    let error = WebhookConfig::new("signed.lua", ["x-signature".to_string()])
        .unwrap()
        .with_signature(signature)
        .unwrap_err();
    assert!(error.contains("must not also be forwarded"));

    let signature =
        WebhookSignatureConfig::hmac_sha256("authorization", "WEBHOOK_SECRET", "sha256=").unwrap();
    let error = WebhookConfig::new("signed.lua", [])
        .unwrap()
        .with_signature(signature)
        .unwrap_err();
    assert!(error.contains("reserved"));
}

#[test]
fn detailed_config_parses_signature_policy() {
    let config: WebhookConfig = noyalib::compat::serde_yaml::from_str(
        r#"
flow: signed.lua
signature:
  type: hmac_sha256
  header: x-hub-signature-256
  secret_env: GITHUB_WEBHOOK_SECRET
  prefix: sha256=
"#,
    )
    .unwrap();

    assert_eq!(config.signature().unwrap().header(), "x-hub-signature-256");
}

#[test]
fn runtime_validation_fails_when_a_signature_secret_is_missing() {
    const MISSING_ENV: &str = "IRONFLOW_TEST_IF098_MISSING_SIGNATURE_SECRET_6F7A";
    let signature =
        WebhookSignatureConfig::hmac_sha256("x-signature", MISSING_ENV, "sha256=").unwrap();
    let config = WebhookConfig::new("signed.lua", [])
        .unwrap()
        .with_signature(signature)
        .unwrap();

    let error = validate_runtime_configs(&HashMap::from([("signed".to_string(), config)]))
        .unwrap_err()
        .to_string();

    assert!(error.contains("signed"));
    assert!(error.contains(MISSING_ENV));
}
