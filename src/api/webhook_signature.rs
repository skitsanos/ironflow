use axum::http::{HeaderMap, HeaderName};
use serde::Deserialize;

const MIN_SECRET_BYTES: usize = 16;
const MAX_PREFIX_BYTES: usize = 64;

/// Fail-closed authentication policy for a named webhook route.
///
/// The current policy verifies an HMAC-SHA256 hex digest over the exact request
/// body before JSON parsing or run creation. Its secret is resolved from the
/// process environment and is never added to workflow context.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WebhookSignatureConfig {
    #[serde(rename = "type")]
    kind: SignatureKind,
    #[serde(deserialize_with = "deserialize_header_name")]
    header: HeaderName,
    secret_env: String,
    #[serde(default = "default_sha256_prefix")]
    prefix: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SignatureKind {
    HmacSha256,
}

#[derive(Debug)]
pub(crate) enum SignatureVerificationError {
    Rejected,
    Misconfigured(String),
}

impl WebhookSignatureConfig {
    /// Create a body-only HMAC-SHA256 policy.
    ///
    /// `header` names the request header containing the hex digest,
    /// `secret_env` names an environment variable containing at least 16 bytes,
    /// and `prefix` is removed before hex decoding (for example, `sha256=`).
    pub fn hmac_sha256(
        header: impl AsRef<str>,
        secret_env: impl Into<String>,
        prefix: impl Into<String>,
    ) -> Result<Self, String> {
        let header = header
            .as_ref()
            .parse::<HeaderName>()
            .map_err(|_| "invalid webhook signature header".to_string())?;
        let config = Self {
            kind: SignatureKind::HmacSha256,
            header,
            secret_env: secret_env.into(),
            prefix: prefix.into(),
        };
        config.validate()?;
        Ok(config)
    }

    /// Return the normalized signature header name.
    pub fn header(&self) -> &str {
        self.header.as_str()
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.secret_env.is_empty()
            || !self
                .secret_env
                .bytes()
                .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
        {
            return Err("webhook signature secret_env must be an environment variable name".into());
        }
        if self.prefix.len() > MAX_PREFIX_BYTES || !self.prefix.is_ascii() {
            return Err(format!(
                "webhook signature prefix must be ASCII and at most {MAX_PREFIX_BYTES} bytes"
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_runtime(&self) -> Result<(), String> {
        self.resolve_secret().map(|_| ())
    }

    pub(crate) fn verify(
        &self,
        headers: &HeaderMap,
        body: &[u8],
    ) -> Result<(), SignatureVerificationError> {
        let secret = self
            .resolve_secret()
            .map_err(SignatureVerificationError::Misconfigured)?;
        let mut values = headers.get_all(&self.header).iter();
        let supplied = values.next().ok_or(SignatureVerificationError::Rejected)?;
        if values.next().is_some() {
            return Err(SignatureVerificationError::Rejected);
        }
        let supplied = supplied
            .to_str()
            .map_err(|_| SignatureVerificationError::Rejected)?;
        let encoded = supplied
            .strip_prefix(&self.prefix)
            .ok_or(SignatureVerificationError::Rejected)?;
        let supplied = hex::decode(encoded).map_err(|_| SignatureVerificationError::Rejected)?;
        let expected = match self.kind {
            SignatureKind::HmacSha256 => {
                crate::util::authentication::hmac_sha256(secret.as_bytes(), body)
            }
        };
        if crate::util::authentication::constant_time_eq(&supplied, &expected) {
            Ok(())
        } else {
            Err(SignatureVerificationError::Rejected)
        }
    }

    fn resolve_secret(&self) -> Result<String, String> {
        let value = std::env::var(&self.secret_env).map_err(|_| {
            format!(
                "webhook signature environment variable '{}' is not configured",
                self.secret_env
            )
        })?;
        if value.len() < MIN_SECRET_BYTES {
            return Err(format!(
                "webhook signature environment variable '{}' must contain at least {MIN_SECRET_BYTES} bytes",
                self.secret_env
            ));
        }
        Ok(value)
    }
}

fn default_sha256_prefix() -> String {
    "sha256=".to_string()
}

fn deserialize_header_name<'de, D>(deserializer: D) -> Result<HeaderName, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    value.parse().map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_environment_names_and_prefixes() {
        assert!(WebhookSignatureConfig::hmac_sha256("x-signature", "", "sha256=").is_err());
        assert!(
            WebhookSignatureConfig::hmac_sha256("x-signature", "SECRET-NAME", "sha256=").is_err()
        );
        assert!(WebhookSignatureConfig::hmac_sha256("x-signature", "SECRET_NAME", "é").is_err());
    }

    #[test]
    fn deserialization_denies_unknown_algorithms() {
        let error = noyalib::compat::serde_yaml::from_str::<WebhookSignatureConfig>(
            "type: hmac_sha1\nheader: x-signature\nsecret_env: SECRET_NAME\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown variant"));
    }
}
