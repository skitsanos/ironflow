use anyhow::Result;

use crate::artifacts::FileSource;
use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;
use crate::util::file_source::parse_file_source;

pub(super) enum WriteInput {
    Text(String),
    Base64 { encoded: String, decoded: u64 },
    Artifact(FileSource),
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
        return Ok(WriteInput::Artifact(parse_file_source(
            value,
            ctx,
            "write_file",
        )?));
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
        return Ok(WriteInput::Artifact(parse_file_source(
            value,
            ctx,
            "write_file",
        )?));
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
            admit_size(text.len() as u64, maximum)?;
            Ok(WriteInput::Text(text))
        }
        "base64" => {
            let decoded = preflight_base64(&text, maximum)?;
            Ok(WriteInput::Base64 {
                encoded: text,
                decoded,
            })
        }
        other => anyhow::bail!(
            "write_file: unsupported encoding '{other}'. Must be 'text', 'base64', or 'artifact'."
        ),
    }
}

pub(super) fn preflight_base64(encoded: &str, maximum: u64) -> Result<u64> {
    let length = encoded.len();
    let remainder = length % 4;
    if remainder == 1 {
        anyhow::bail!("write_file: base64 input has an invalid encoded length");
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
    admit_size(decoded, maximum)?;
    Ok(decoded)
}

fn admit_size(size: u64, maximum: u64) -> Result<()> {
    if size > maximum {
        anyhow::bail!(
            "write_file: final payload is {size} bytes, exceeds IRONFLOW_MAX_FILE_BYTES ({maximum})"
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
