use std::collections::HashSet;

use axum::http::{HeaderMap, HeaderName};
use serde::Deserialize;

use crate::engine::types::Context;

const EXECUTION_HEADERS_KEY: &str = "_headers";
const MIN_CONFIDENTIAL_HEADER_BYTES: usize = 8;

/// Headers owned by IronFlow or by an upstream authentication layer.
///
/// These values may authorize access to the IronFlow API itself, so a webhook
/// must never receive them as workflow input even when a route is otherwise
/// configured to forward request headers.
const RESERVED_CREDENTIAL_HEADERS: &[&str] = &[
    "authorization",
    "cf-access-client-secret",
    "cf-access-jwt-assertion",
    "cookie",
    "proxy-authorization",
    "set-cookie",
    "x-amz-security-token",
    "x-api-key",
    "x-auth-token",
    "x-auth-request-access-token",
    "x-forwarded-authorization",
    "x-goog-api-key",
    "x-session-token",
];

/// One named webhook route.
///
/// The string form remains supported for existing configurations. The object
/// form opts selected business headers into the execution-only `_headers`
/// overlay:
///
/// ```yaml
/// webhooks:
///   legacy: legacy.lua
///   signed:
///     flow: signed.lua
///     forward_headers:
///       - stripe-signature
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebhookConfig {
    flow: String,
    forward_headers: Vec<HeaderName>,
}

impl WebhookConfig {
    pub fn new(
        flow: impl Into<String>,
        forward_headers: impl IntoIterator<Item = String>,
    ) -> Result<Self, String> {
        let flow = flow.into();
        if flow.trim().is_empty() {
            return Err("webhook flow path must not be empty".to_string());
        }

        let mut seen = HashSet::new();
        let mut normalized = Vec::new();
        for raw_name in forward_headers {
            let name = raw_name
                .parse::<HeaderName>()
                .map_err(|_| format!("invalid webhook forward header '{raw_name}'"))?;
            let lower = name.as_str();
            if RESERVED_CREDENTIAL_HEADERS.contains(&lower) {
                return Err(format!(
                    "webhook header '{lower}' is reserved for platform or transport authentication"
                ));
            }
            if seen.insert(name.clone()) {
                normalized.push(name);
            }
        }

        Ok(Self {
            flow,
            forward_headers: normalized,
        })
    }

    pub fn flow(&self) -> &str {
        &self.flow
    }

    pub fn forward_headers(&self) -> impl Iterator<Item = &str> {
        self.forward_headers.iter().map(HeaderName::as_str)
    }

    /// Build the invocation-only context overlay for a request.
    ///
    /// Only explicitly configured names are copied. Duplicate or non-text
    /// values are rejected so signature validation never sees an ambiguous or
    /// silently missing credential.
    pub(crate) fn execution_overlay(&self, headers: &HeaderMap) -> Result<Context, String> {
        let mut forwarded = serde_json::Map::new();
        for name in &self.forward_headers {
            let mut values = headers.get_all(name).iter();
            let Some(value) = values.next() else {
                continue;
            };
            if values.next().is_some() {
                return Err(format!(
                    "forwarded webhook header '{}' must not be repeated",
                    name.as_str()
                ));
            }
            let value = value.to_str().map_err(|_| {
                format!(
                    "forwarded webhook header '{}' must contain visible text",
                    name.as_str()
                )
            })?;
            let non_whitespace_bytes = value
                .bytes()
                .filter(|byte| !byte.is_ascii_whitespace())
                .count();
            if non_whitespace_bytes < MIN_CONFIDENTIAL_HEADER_BYTES {
                return Err(format!(
                    "forwarded webhook header '{}' must contain at least {MIN_CONFIDENTIAL_HEADER_BYTES} non-whitespace bytes",
                    name.as_str()
                ));
            }
            forwarded.insert(
                name.as_str().to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }

        Ok(Context::from([(
            EXECUTION_HEADERS_KEY.to_string(),
            serde_json::Value::Object(forwarded),
        )]))
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawWebhookConfig {
    Flow(String),
    Detailed(RawDetailedWebhookConfig),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDetailedWebhookConfig {
    flow: String,
    #[serde(default)]
    forward_headers: Vec<String>,
}

impl<'de> Deserialize<'de> for WebhookConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawWebhookConfig::deserialize(deserializer)?;
        let (flow, forward_headers) = match raw {
            RawWebhookConfig::Flow(flow) => (flow, Vec::new()),
            RawWebhookConfig::Detailed(RawDetailedWebhookConfig {
                flow,
                forward_headers,
            }) => (flow, forward_headers),
        };
        Self::new(flow, forward_headers).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
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
}
