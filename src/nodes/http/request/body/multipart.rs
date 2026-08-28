use anyhow::Result;
use reqwest::multipart::{Form, Part};
use serde_json::{Map, Value};

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

use super::{ArtifactUpload, RequestBody, artifact_upload, open_artifact};

const MAX_MULTIPART_PARTS: usize = 100;
const MAX_PART_METADATA_BYTES: usize = 255;
const FINAL_BOUNDARY_ALLOWANCE: u64 = 128;
const PART_FRAMING_ALLOWANCE: u64 = 256;

#[derive(Clone, Debug)]
pub(in crate::nodes::http::request) enum MultipartField {
    Text {
        name: String,
        value: String,
    },
    Artifact {
        name: String,
        upload: ArtifactUpload,
        filename: String,
        mime_type: Option<String>,
    },
}

pub(super) fn resolve(config: &Value, ctx: &Context) -> Result<RequestBody> {
    if config.get("body").is_some() || config.get("body_key").is_some() {
        anyhow::bail!("HTTP multipart requests use 'parts', not 'body' or 'body_key'");
    }
    let parts = config
        .get("parts")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("HTTP body_type='multipart' requires a 'parts' array"))?;
    if parts.is_empty() || parts.len() > MAX_MULTIPART_PARTS {
        anyhow::bail!("HTTP multipart parts count must be between 1 and {MAX_MULTIPART_PARTS}");
    }
    parts
        .iter()
        .map(|part| resolve_part(part, ctx))
        .collect::<Result<Vec<_>>>()
        .map(RequestBody::Multipart)
}

fn resolve_part(value: &Value, ctx: &Context) -> Result<MultipartField> {
    let part = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("HTTP multipart part must be an object"))?;
    reject_unknown_fields(part)?;
    let name = required_metadata(part, "name")?;
    let text = optional_string(part, "text")?;
    let source_key = optional_string(part, "source_key")?;
    let direct = part.get("artifact");
    let forms = usize::from(text.is_some())
        + usize::from(source_key.is_some())
        + usize::from(direct.is_some());
    if forms != 1 {
        anyhow::bail!(
            "HTTP multipart part requires exactly one of 'text', 'source_key', or 'artifact'"
        );
    }
    if let Some(text) = text {
        if part.contains_key("filename") || part.contains_key("content_type") {
            anyhow::bail!("HTTP multipart text parts cannot set filename or content_type");
        }
        return Ok(MultipartField::Text {
            name,
            value: interpolate_ctx(text, ctx),
        });
    }

    let source = match source_key {
        Some(key) => ctx.get(&interpolate_ctx(key, ctx)).ok_or_else(|| {
            anyhow::anyhow!("HTTP multipart source_key '{key}' not found in context")
        })?,
        None => direct.expect("exclusive source validation ensures artifact exists"),
    };
    let upload = artifact_upload(source)?;
    let filename =
        optional_metadata(part, "filename")?.unwrap_or_else(|| upload.source.file_name());
    let configured_mime = optional_metadata(part, "content_type")?;
    if let (Some(descriptor), Some(configured)) = (&upload.mime_type, &configured_mime)
        && descriptor != configured
    {
        anyhow::bail!(
            "HTTP multipart content_type '{configured}' conflicts with artifact MIME type '{descriptor}'"
        );
    }
    Ok(MultipartField::Artifact {
        name,
        mime_type: configured_mime.or_else(|| upload.mime_type.clone()),
        upload,
        filename,
    })
}

pub(super) async fn build(fields: &[MultipartField]) -> Result<Form> {
    let maximum = crate::util::limits::max_http_body_bytes();
    let mut admitted = admit_bytes(0, FINAL_BOUNDARY_ALLOWANCE, maximum)?;
    let mut form = Form::new();
    for field in fields {
        admitted = admit_bytes(admitted, framing_allowance(field)?, maximum)?;
        match field {
            MultipartField::Text { name, value } => {
                admitted = admit_bytes(admitted, value.len() as u64, maximum)?;
                form = form.text(name.clone(), value.clone());
            }
            MultipartField::Artifact {
                name,
                upload,
                filename,
                mime_type,
            } => {
                let opened = open_artifact(upload).await?;
                admitted = admit_bytes(admitted, opened.size, maximum)?;
                let mut part = Part::stream_with_length(
                    reqwest::Body::from(tokio::fs::File::from_std(opened.file)),
                    opened.size,
                )
                .file_name(filename.clone());
                if let Some(mime_type) = mime_type {
                    part = part.mime_str(mime_type)?;
                }
                form = form.part(name.clone(), part);
            }
        }
    }
    Ok(form)
}

fn framing_allowance(field: &MultipartField) -> Result<u64> {
    let metadata_bytes = match field {
        MultipartField::Text { name, .. } => name.len(),
        MultipartField::Artifact {
            name,
            filename,
            mime_type,
            ..
        } => name
            .len()
            .checked_add(filename.len())
            .and_then(|value| value.checked_add(mime_type.as_deref().map_or(0, str::len)))
            .ok_or_else(|| anyhow::anyhow!("HTTP multipart metadata size overflow"))?,
    };
    let escaped = u64::try_from(metadata_bytes)?
        .checked_mul(3)
        .ok_or_else(|| anyhow::anyhow!("HTTP multipart metadata size overflow"))?;
    PART_FRAMING_ALLOWANCE
        .checked_add(escaped)
        .ok_or_else(|| anyhow::anyhow!("HTTP multipart framing size overflow"))
}

fn admit_bytes(current: u64, additional: u64, maximum: u64) -> Result<u64> {
    let total = current
        .checked_add(additional)
        .ok_or_else(|| anyhow::anyhow!("HTTP multipart payload size overflow"))?;
    if total > maximum {
        anyhow::bail!("HTTP multipart payload exceeds IRONFLOW_MAX_HTTP_BODY_BYTES ({maximum})");
    }
    Ok(total)
}

fn reject_unknown_fields(part: &Map<String, Value>) -> Result<()> {
    for key in part.keys() {
        if !matches!(
            key.as_str(),
            "name" | "text" | "source_key" | "artifact" | "filename" | "content_type"
        ) {
            anyhow::bail!("HTTP multipart part has unknown field '{key}'");
        }
    }
    Ok(())
}

fn required_metadata(part: &Map<String, Value>, key: &str) -> Result<String> {
    optional_metadata(part, key)?
        .ok_or_else(|| anyhow::anyhow!("HTTP multipart part requires non-empty '{key}'"))
}

fn optional_metadata(part: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    let value = optional_string(part, key)?;
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty()
        || value.len() > MAX_PART_METADATA_BYTES
        || value.bytes().any(|byte| byte == b'\r' || byte == b'\n')
    {
        anyhow::bail!(
            "HTTP multipart '{key}' must be 1 to {MAX_PART_METADATA_BYTES} bytes without CR/LF"
        );
    }
    if key == "content_type" {
        crate::artifacts::validate_mime_type(Some(value))?;
    }
    Ok(Some(value.to_owned()))
}

fn optional_string<'a>(part: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>> {
    match part.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => anyhow::bail!("HTTP multipart '{key}' must be a string"),
    }
}
