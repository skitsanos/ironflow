use std::collections::{HashMap, HashSet};

use axum::http::{HeaderMap, HeaderName};
use serde::Deserialize;

use crate::engine::types::Context;

use super::webhook_signature::{SignatureVerificationError, WebhookSignatureConfig};

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
    signature: Option<WebhookSignatureConfig>,
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
            signature: None,
        })
    }

    /// Attach a fail-closed request-signature policy to this route.
    ///
    /// The signature header cannot also be forwarded to workflow context.
    pub fn with_signature(mut self, signature: WebhookSignatureConfig) -> Result<Self, String> {
        signature.validate()?;
        let header = signature.header();
        if RESERVED_CREDENTIAL_HEADERS.contains(&header) {
            return Err(format!(
                "webhook signature header '{header}' is reserved for platform or transport authentication"
            ));
        }
        if self.forward_headers().any(|forwarded| forwarded == header) {
            return Err(format!(
                "webhook signature header '{header}' must not also be forwarded to the workflow"
            ));
        }
        self.signature = Some(signature);
        Ok(self)
    }

    pub fn flow(&self) -> &str {
        &self.flow
    }

    pub fn forward_headers(&self) -> impl Iterator<Item = &str> {
        self.forward_headers.iter().map(HeaderName::as_str)
    }

    /// Return the route's request-signature policy, if configured.
    pub fn signature(&self) -> Option<&WebhookSignatureConfig> {
        self.signature.as_ref()
    }

    pub(crate) fn validate_runtime(&self) -> Result<(), String> {
        self.signature
            .as_ref()
            .map_or(Ok(()), WebhookSignatureConfig::validate_runtime)
    }

    pub(crate) fn verify_signature(
        &self,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Result<(), SignatureVerificationError> {
        self.signature
            .as_ref()
            .map_or(Ok(()), |signature| signature.verify(headers, body))
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
    signature: Option<WebhookSignatureConfig>,
}

impl<'de> Deserialize<'de> for WebhookConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawWebhookConfig::deserialize(deserializer)?;
        let (flow, forward_headers, signature) = match raw {
            RawWebhookConfig::Flow(flow) => (flow, Vec::new(), None),
            RawWebhookConfig::Detailed(RawDetailedWebhookConfig {
                flow,
                forward_headers,
                signature,
            }) => (flow, forward_headers, signature),
        };
        let config = Self::new(flow, forward_headers).map_err(serde::de::Error::custom)?;
        match signature {
            Some(signature) => config
                .with_signature(signature)
                .map_err(serde::de::Error::custom),
            None => Ok(config),
        }
    }
}

pub(crate) fn validate_runtime_configs(
    configs: &HashMap<String, WebhookConfig>,
) -> anyhow::Result<()> {
    for (name, webhook) in configs {
        webhook.validate_runtime().map_err(|error| {
            anyhow::anyhow!("invalid webhook signature configuration for '{name}': {error}")
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
