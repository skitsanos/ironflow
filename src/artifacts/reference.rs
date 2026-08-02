use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const URI_PREFIX: &str = "artifact://sha256/";
const SHA256_HEX_LENGTH: usize = 64;
const MAX_MIME_TYPE_BYTES: usize = 255;

/// A serializable reference to immutable content held by an artifact store.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub artifact_uri: String,
    pub sha256: String,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

impl ArtifactRef {
    pub(crate) fn validate_uri(uri: &str) -> Result<()> {
        digest_from_uri(uri).map(|_| ())
    }

    pub(crate) fn from_digest(
        sha256: String,
        size_bytes: u64,
        mime_type: Option<String>,
    ) -> Result<Self> {
        let artifact_uri = format!("{URI_PREFIX}{sha256}");
        let artifact = Self {
            artifact_uri,
            sha256,
            size_bytes,
            mime_type,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    /// Parse a descriptor without cloning unrelated JSON fields.
    pub(crate) fn from_value(value: &Value) -> Result<Self> {
        let object = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("artifact descriptor must be an object"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "artifact_uri" | "sha256" | "size_bytes" | "mime_type"
            ) {
                bail!("unknown artifact descriptor field '{key}'");
            }
        }
        let required_string = |key: &str| -> Result<String> {
            object
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| {
                    anyhow::anyhow!("artifact descriptor field '{key}' must be a string")
                })
        };
        let mime_type = match object.get("mime_type") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => Some(value.clone()),
            Some(_) => bail!("artifact descriptor field 'mime_type' must be a string or null"),
        };
        let artifact = Self {
            artifact_uri: required_string("artifact_uri")?,
            sha256: required_string("sha256")?,
            size_bytes: object
                .get("size_bytes")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "artifact descriptor field 'size_bytes' must be a non-negative integer"
                    )
                })?,
            mime_type,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    /// Validate that the descriptor uses the canonical SHA-256 artifact URI.
    pub fn validate(&self) -> Result<()> {
        validate_digest(&self.sha256)?;
        let uri_digest = digest_from_uri(&self.artifact_uri)?;
        if uri_digest != self.sha256 {
            bail!("artifact URI digest does not match its sha256 field");
        }
        validate_mime_type(self.mime_type.as_deref())?;
        Ok(())
    }
}

pub(crate) fn digest_from_uri(uri: &str) -> Result<&str> {
    let Some(digest) = uri.strip_prefix(URI_PREFIX) else {
        bail!("artifact URI must start with '{URI_PREFIX}'");
    };
    validate_digest(digest)?;
    Ok(digest)
}

fn validate_digest(digest: &str) -> Result<()> {
    if digest.len() != SHA256_HEX_LENGTH
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("artifact SHA-256 digest must be exactly 64 lowercase hexadecimal characters");
    }
    Ok(())
}

pub(crate) fn validate_mime_type(mime_type: Option<&str>) -> Result<()> {
    let Some(mime_type) = mime_type else {
        return Ok(());
    };
    if mime_type.is_empty()
        || mime_type.len() > MAX_MIME_TYPE_BYTES
        || mime_type.trim() != mime_type
        || !mime_type
            .bytes()
            .all(|byte| byte == b' ' || byte.is_ascii_graphic())
    {
        bail!(
            "artifact MIME type must be 1 to {MAX_MIME_TYPE_BYTES} visible ASCII bytes without surrounding whitespace"
        );
    }
    Ok(())
}
