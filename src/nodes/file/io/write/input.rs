use std::path::PathBuf;

use anyhow::Result;
use base64::engine::GeneralPurpose;
use base64::engine::general_purpose::STANDARD;

use crate::artifacts::FileSource;
use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;
use crate::util::file_source::{SourceOrigin, parse_artifact_source};

pub(super) enum WriteInput {
    Text(String),
    Base64 {
        encoded: String,
        decoded: u64,
        engine: &'static GeneralPurpose,
    },
    Artifact(FileSource),
    /// A regular file opened with the `read_file` source policy (no final
    /// symlink, no special file) and admitted against the byte limit.
    File(PathBuf),
}

pub(super) fn parse_input(
    config: &serde_json::Value,
    ctx: &Context,
    maximum: u64,
) -> Result<WriteInput> {
    let forms = ["content", "source_key", "artifact"]
        .into_iter()
        .filter(|key| config.get(*key).is_some())
        .count();
    if forms > 1 {
        anyhow::bail!("write_file accepts exactly one of 'content', 'source_key', or 'artifact'");
    }
    let encoding = optional_string(config, "encoding")?.unwrap_or("text");
    if let Some(value) = config.get("artifact") {
        if encoding != "text" && encoding != "artifact" {
            anyhow::bail!("write_file: artifact input is incompatible with encoding '{encoding}'");
        }
        return artifact_input(value, SourceOrigin::Config(ctx));
    }

    let (value, interpolate) = if let Some(key) = optional_string(config, "source_key")? {
        (
            ctx.get(key)
                .ok_or_else(|| anyhow::anyhow!("Key '{key}' not found in context"))?,
            false,
        )
    } else if let Some(value) = config.get("content") {
        (value, true)
    } else {
        return Ok(WriteInput::Text(String::new()));
    };
    if value.is_object() || encoding == "artifact" {
        let origin = if interpolate {
            SourceOrigin::Config(ctx)
        } else {
            SourceOrigin::Data
        };
        return artifact_input(value, origin);
    }
    let text = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("write_file content source must be a string or artifact"))?;
    let text = if interpolate {
        interpolate_ctx(text, ctx)
    } else {
        text.to_owned()
    };
    match encoding {
        "text" => {
            admit_size(text.len() as u64, maximum, "write_file")?;
            Ok(WriteInput::Text(text))
        }
        "base64" => {
            let decoded = preflight_base64(&text, maximum, "write_file")?;
            Ok(WriteInput::Base64 {
                encoded: text,
                decoded,
                engine: &STANDARD,
            })
        }
        other => anyhow::bail!(
            "write_file: unsupported encoding '{other}'. Must be 'text', 'base64', or 'artifact'."
        ),
    }
}

/// An artifact input names stored bytes by descriptor or canonical URI. A
/// local path is refused here: `write_file` copies files only through
/// `copy_file`, which applies the source-file policy.
fn artifact_input(value: &serde_json::Value, origin: SourceOrigin<'_>) -> Result<WriteInput> {
    parse_artifact_source(value, origin)
        .map(WriteInput::Artifact)
        .map_err(|error| {
            anyhow::anyhow!(
                "write_file: artifact input must be an artifact descriptor or a canonical artifact://sha256/... URI: {error}"
            )
        })
}

/// Compute the decoded length of `encoded` from its encoded length alone and
/// admit it against `maximum`, so no decoded byte is allocated for an
/// oversized payload.
pub(super) fn preflight_base64(encoded: &str, maximum: u64, operation: &str) -> Result<u64> {
    let length = encoded.len();
    let remainder = length % 4;
    if remainder == 1 {
        anyhow::bail!("{operation}: base64 input has an invalid encoded length");
    }
    let mut decoded = (length / 4)
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add([0, 0, 1, 2][remainder]))
        .and_then(|bytes| u64::try_from(bytes).ok())
        .unwrap_or(u64::MAX);
    if remainder == 0 {
        decoded = decoded.saturating_sub(
            encoded
                .as_bytes()
                .iter()
                .rev()
                .take(2)
                .take_while(|byte| **byte == b'=')
                .count() as u64,
        );
    }
    admit_size(decoded, maximum, operation)?;
    Ok(decoded)
}

pub(super) fn admit_size(size: u64, maximum: u64, operation: &str) -> Result<()> {
    if size > maximum {
        anyhow::bail!(
            "{operation}: final payload is {size} bytes, exceeds the IRONFLOW_MAX_FILE_BYTES limit ({maximum})"
        );
    }
    Ok(())
}

fn optional_string<'a>(config: &'a serde_json::Value, key: &str) -> Result<Option<&'a str>> {
    match config.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(_) => anyhow::bail!("write_file: '{key}' must be a string"),
    }
}
