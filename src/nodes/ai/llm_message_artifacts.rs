use anyhow::Result;
use base64::Engine;
use serde_json::{Map, Value, json};

use crate::artifacts::{ArtifactRef, FileSource};
use crate::engine::types::Context;
use crate::util::bounded_read::read_capped_controlled;
use crate::util::execution::run_tracked_blocking_step;
use crate::util::limits;
use crate::util::node_config::{config_u64_strict, config_usize_strict};

struct PendingImage {
    message_index: usize,
    block_index: usize,
    source: FileSource,
    declared_mime: Option<String>,
    detail: Option<String>,
}

struct ResolvedImage {
    message_index: usize,
    block_index: usize,
    data_url: String,
    detail: Option<String>,
}

pub(super) async fn resolve(
    mut messages: Vec<Value>,
    config: &Value,
    ctx: &Context,
) -> Result<(Vec<Value>, bool)> {
    let pending = collect(&messages, ctx)?;
    if pending.is_empty() {
        return Ok((messages, false));
    }

    let count_limit = configured_count_limit(config, ctx)?;
    if pending.len() > count_limit {
        anyhow::bail!(
            "llm: {} image_artifact blocks exceed max_image_artifacts limit of {}",
            pending.len(),
            count_limit
        );
    }
    let byte_limit = configured_byte_limit(config, ctx)?;
    let resolved = run_tracked_blocking_step(move |execution| {
        let mut used = 0_u64;
        let mut output = Vec::with_capacity(pending.len());
        for image in pending {
            execution.checkpoint()?;
            let remaining = byte_limit.saturating_sub(used);
            if remaining == 0 {
                anyhow::bail!(
                    "llm image artifacts exceed max_image_input_bytes limit of {byte_limit}"
                );
            }
            let (file, _) = image
                .source
                .open("llm image artifact", &execution)?
                .into_parts();
            let bytes = read_capped_controlled(file, remaining, "llm image artifacts", &execution)?;
            used = used.saturating_add(bytes.len() as u64);
            let mime = validated_mime(&bytes, image.declared_mime.as_deref())?;
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            output.push(ResolvedImage {
                message_index: image.message_index,
                block_index: image.block_index,
                data_url: format!("data:{mime};base64,{encoded}"),
                detail: image.detail,
            });
        }
        Ok(output)
    })
    .await?;

    for image in resolved {
        let block = messages
            .get_mut(image.message_index)
            .and_then(Value::as_object_mut)
            .and_then(|message| message.get_mut("content"))
            .and_then(Value::as_array_mut)
            .and_then(|content| content.get_mut(image.block_index))
            .expect("validated image block path must remain present");
        let mut image_url = Map::from_iter([("url".to_string(), Value::String(image.data_url))]);
        if let Some(detail) = image.detail {
            image_url.insert("detail".to_string(), Value::String(detail));
        }
        *block = json!({
            "type": "image_url",
            "image_url": Value::Object(image_url),
        });
    }
    Ok((messages, true))
}

pub(super) fn redact_data_urls(input: &str) -> String {
    const PREFIX: &str = "data:image/";
    const MARKER: &str = ";base64,";
    const REDACTED: &str = "<redacted image data URL>";

    let mut remaining = input;
    let mut output = String::with_capacity(input.len());
    while let Some(start) = remaining.find(PREFIX) {
        output.push_str(&remaining[..start]);
        let candidate = &remaining[start..];
        let Some(marker_start) = candidate.find(MARKER).filter(|position| *position <= 64) else {
            output.push_str(PREFIX);
            remaining = &candidate[PREFIX.len()..];
            continue;
        };
        let payload_start = marker_start + MARKER.len();
        let payload_len = candidate[payload_start..]
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'+' | b'/' | b'='))
            .count();
        output.push_str(REDACTED);
        remaining = &candidate[payload_start + payload_len..];
    }
    output.push_str(remaining);
    output
}

fn collect(messages: &[Value], ctx: &Context) -> Result<Vec<PendingImage>> {
    let mut pending = Vec::new();
    for (message_index, message) in messages.iter().enumerate() {
        let Some(content) = message
            .as_object()
            .and_then(|message| message.get("content"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for (block_index, block) in content.iter().enumerate() {
            let Some(object) = block.as_object() else {
                continue;
            };
            if object.get("type").and_then(Value::as_str) != Some("image_artifact") {
                continue;
            }
            pending.push(parse_block(object, ctx, message_index, block_index)?);
        }
    }
    Ok(pending)
}

fn parse_block(
    block: &Map<String, Value>,
    ctx: &Context,
    message_index: usize,
    block_index: usize,
) -> Result<PendingImage> {
    for key in block.keys() {
        if !matches!(
            key.as_str(),
            "type" | "source_key" | "artifact" | "mime_type" | "detail"
        ) {
            anyhow::bail!("llm: image_artifact block has unknown field '{key}'");
        }
    }
    let source_key = optional_string(block, "source_key")?;
    let direct = block.get("artifact");
    if source_key.is_some() == direct.is_some() {
        anyhow::bail!(
            "llm: image_artifact block requires exactly one of 'source_key' or 'artifact'"
        );
    }
    let value = if let Some(source_key) = source_key {
        ctx.get(source_key).ok_or_else(|| {
            anyhow::anyhow!("llm: image_artifact source_key '{source_key}' not found in context")
        })?
    } else {
        direct.expect("exclusive source validation ensures artifact is present")
    };
    let (source, descriptor_mime) = artifact_source(value)?;
    let configured_mime = optional_string(block, "mime_type")?.map(str::to_owned);
    if let (Some(descriptor), Some(configured)) = (&descriptor_mime, &configured_mime)
        && descriptor != configured
    {
        anyhow::bail!(
            "llm: image_artifact MIME type '{configured}' conflicts with descriptor MIME type '{descriptor}'"
        );
    }
    let detail = optional_string(block, "detail")?.map(str::to_owned);
    if let Some(detail) = detail.as_deref()
        && !matches!(detail, "auto" | "low" | "high")
    {
        anyhow::bail!("llm: image_artifact detail must be 'auto', 'low', or 'high'");
    }
    Ok(PendingImage {
        message_index,
        block_index,
        source,
        declared_mime: configured_mime.or(descriptor_mime),
        detail,
    })
}

fn artifact_source(value: &Value) -> Result<(FileSource, Option<String>)> {
    match value {
        Value::String(uri) => {
            ArtifactRef::validate_uri(uri)?;
            Ok((FileSource::artifact_uri(uri), None))
        }
        Value::Object(object) if object.len() == 1 && object.contains_key("artifact") => {
            artifact_source(&object["artifact"])
        }
        Value::Object(_) => {
            let artifact = ArtifactRef::from_value(value)
                .map_err(|error| anyhow::anyhow!("llm: invalid image artifact: {error}"))?;
            let mime = artifact.mime_type.clone();
            Ok((FileSource::artifact(artifact), mime))
        }
        _ => {
            anyhow::bail!("llm: image_artifact source must be an artifact URI or descriptor object")
        }
    }
}

fn optional_string<'a>(object: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value)),
        Some(_) => anyhow::bail!("llm: image_artifact '{key}' must be a non-empty string"),
    }
}

fn configured_count_limit(config: &Value, ctx: &Context) -> Result<usize> {
    let process_limit = limits::max_llm_image_artifacts();
    let requested =
        config_usize_strict(config, "max_image_artifacts", ctx)?.unwrap_or(process_limit);
    if requested == 0 || requested > process_limit {
        anyhow::bail!(
            "llm: max_image_artifacts must be between 1 and process limit {process_limit}"
        );
    }
    Ok(requested)
}

fn configured_byte_limit(config: &Value, ctx: &Context) -> Result<u64> {
    let process_limit = limits::max_llm_image_input_bytes();
    let requested =
        config_u64_strict(config, "max_image_input_bytes", ctx)?.unwrap_or(process_limit);
    if requested == 0 || requested > process_limit {
        anyhow::bail!(
            "llm: max_image_input_bytes must be between 1 and process limit {process_limit}"
        );
    }
    Ok(requested)
}

fn validated_mime(bytes: &[u8], declared: Option<&str>) -> Result<&'static str> {
    let detected = match image::guess_format(bytes) {
        Ok(image::ImageFormat::Png) => "image/png",
        Ok(image::ImageFormat::Jpeg) => "image/jpeg",
        Ok(image::ImageFormat::WebP) => "image/webp",
        Ok(image::ImageFormat::Gif) => "image/gif",
        Ok(other) => anyhow::bail!(
            "llm: image_artifact format {other:?} is not supported; use PNG, JPEG, WebP, or GIF"
        ),
        Err(error) => {
            anyhow::bail!("llm: image_artifact bytes are not a recognized image: {error}")
        }
    };
    if let Some(declared) = declared
        && declared != detected
    {
        anyhow::bail!(
            "llm: image_artifact declared MIME type '{declared}' does not match detected '{detected}'"
        );
    }
    Ok(detected)
}
