use anyhow::Result;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

#[derive(Debug)]
pub(crate) enum ImageInput {
    Path(String),
    Base64(String),
}

#[derive(Debug)]
pub(crate) struct LoadedImage {
    pub(crate) label: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) image: image::DynamicImage,
}

pub(crate) fn resolve_single_image_source(
    config: &serde_json::Value,
    ctx: &Context,
    node_name: &str,
) -> Result<ImageInput> {
    let has_path = config.get("path").and_then(|v| v.as_str()).is_some();
    let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

    if has_path && has_source_key {
        anyhow::bail!(
            "{} accepts either 'path' or 'source_key', not both",
            node_name
        );
    }

    if let Some(path) = config.get("path").and_then(|v| v.as_str()) {
        Ok(ImageInput::Path(interpolate_ctx(path, ctx)))
    } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
        let val = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        parse_image_input(val, ctx)
    } else {
        anyhow::bail!("{} requires either 'path' or 'source_key'", node_name)
    }
}

pub(crate) fn resolve_image_sources(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<Vec<ImageInput>> {
    let has_sources = config.get("sources").is_some();
    let has_source_key = config.get("source_key").is_some();

    if has_sources && has_source_key {
        anyhow::bail!("image_to_pdf accepts either 'sources' or 'source_key', not both");
    }

    let from_config = if let Some(sources) = config.get("sources") {
        sources
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("image_to_pdf: 'sources' must be an array"))?
            .iter()
            .map(|value| parse_image_input(value, ctx))
            .collect::<Result<Vec<_>>>()?
    } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
        let val = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        val.as_array()
            .ok_or_else(|| anyhow::anyhow!("Context key '{}' must be an array", source_key))?
            .iter()
            .map(|value| parse_image_input(value, ctx))
            .collect::<Result<Vec<_>>>()?
    } else {
        anyhow::bail!("image_to_pdf requires either 'sources' or 'source_key'")
    };

    Ok(from_config)
}

pub(crate) fn parse_image_input(value: &serde_json::Value, ctx: &Context) -> Result<ImageInput> {
    if let Some(path) = value.as_str() {
        return Ok(ImageInput::Path(interpolate_ctx(path, ctx)));
    }

    let value = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("image_to_pdf source entries must be strings or objects"))?;

    if let Some(path) = value.get("path").and_then(|v| v.as_str()) {
        Ok(ImageInput::Path(interpolate_ctx(path, ctx)))
    } else if let Some(data) = value.get("base64").and_then(|v| v.as_str()) {
        Ok(ImageInput::Base64(data.to_string()))
    } else if let Some(data) = value.get("data").and_then(|v| v.as_str()) {
        Ok(ImageInput::Base64(data.to_string()))
    } else {
        Err(anyhow::anyhow!(
            "image_to_pdf source object must include 'path' or 'base64'/'data'"
        ))
    }
}
