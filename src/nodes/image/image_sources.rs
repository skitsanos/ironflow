use anyhow::Result;

use crate::engine::types::Context;

use super::resource::ImageToPdfLimits;

mod base64_admission;

use base64_admission::Base64Admission;
pub(crate) use base64_admission::preflight_base64_bytes;

#[derive(Debug)]
pub(crate) enum ImageInput {
    Path(String),
    Base64(String),
}

#[derive(Debug)]
pub(crate) struct LoadedImage {
    pub(crate) image: image::DynamicImage,
}

pub(crate) struct LoadedImageBytes {
    pub(crate) label: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: image::ImageFormat,
    pub(crate) color_type: image::ColorType,
    pub(crate) total_bytes: u64,
    pub(crate) pixels: u64,
}

pub(crate) fn resolve_single_image_source(
    config: &serde_json::Value,
    ctx: &Context,
    node_name: &str,
) -> Result<ImageInput> {
    let maximum = crate::util::limits::max_image_encoded_bytes();
    let mut base64 = Base64Admission::new(maximum, maximum, false);
    let path = config.get("path");
    let source_key = config.get("source_key");
    if path.is_some() && source_key.is_some() {
        anyhow::bail!("{node_name} accepts either 'path' or 'source_key', not both");
    }

    if let Some(path) = path {
        let path = path
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{node_name}: 'path' must be a string"))?;
        return parse_image_input(
            &serde_json::Value::String(path.to_owned()),
            ctx,
            node_name,
            &mut base64,
        );
    }

    if let Some(source_key) = source_key {
        let source_key = source_key
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{node_name}: 'source_key' must be a string"))?;
        let value = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{source_key}' not found in context"))?;
        return parse_image_input(value, ctx, node_name, &mut base64);
    }

    anyhow::bail!("{node_name} requires either 'path' or 'source_key'")
}

pub(crate) fn resolve_image_sources(
    config: &serde_json::Value,
    ctx: &Context,
    limits: ImageToPdfLimits,
) -> Result<Vec<ImageInput>> {
    let sources = config.get("sources");
    let source_key = config.get("source_key");
    if sources.is_some() && source_key.is_some() {
        anyhow::bail!("image_to_pdf accepts either 'sources' or 'source_key', not both");
    }

    let values = if let Some(sources) = sources {
        sources
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("image_to_pdf: 'sources' must be an array"))?
    } else if let Some(source_key) = source_key {
        let source_key = source_key
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("image_to_pdf: 'source_key' must be a string"))?;
        ctx.get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{source_key}' not found in context"))?
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Context key '{source_key}' must be an array"))?
    } else {
        anyhow::bail!("image_to_pdf requires either 'sources' or 'source_key'")
    };

    validate_source_count(values.len(), limits.max_sources)?;
    let mut base64 = Base64Admission::new(
        limits.decode.max_encoded_bytes,
        limits.max_encoded_bytes,
        true,
    );
    let mut inputs = Vec::new();
    inputs
        .try_reserve_exact(values.len())
        .map_err(|error| anyhow::anyhow!("image_to_pdf: cannot reserve source list: {error}"))?;
    for value in values {
        // Base64 length and the cumulative decoded-byte ceiling are admitted
        // inside `parse_image_input` before its context string is cloned.
        inputs.push(parse_image_input(value, ctx, "image_to_pdf", &mut base64)?);
    }
    Ok(inputs)
}

fn parse_image_input(
    value: &serde_json::Value,
    ctx: &Context,
    node_name: &str,
    base64_admission: &mut Base64Admission,
) -> Result<ImageInput> {
    if value.is_string() {
        return Ok(ImageInput::Path(
            crate::util::node_config::resolve_path_value(value, ctx, node_name)?,
        ));
    }

    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("{node_name}: image source must be a string or object"))?;
    let forms = ["path", "base64", "data", "artifact"]
        .into_iter()
        .filter(|key| object.contains_key(*key))
        .count()
        + usize::from(object.contains_key("artifact_uri"));
    if forms == 0 {
        anyhow::bail!(
            "{node_name}: image source object must include exactly one of 'path', 'artifact', 'base64', or 'data'"
        );
    }
    if forms > 1 {
        anyhow::bail!("{node_name}: image source object contains ambiguous source fields");
    }

    if let Some(path) = object.get("path") {
        let path = path
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{node_name}: source 'path' must be a string"))?;
        return Ok(ImageInput::Path(
            crate::util::node_config::resolve_path_value(
                &serde_json::Value::String(path.to_owned()),
                ctx,
                node_name,
            )?,
        ));
    }
    if let Some(data) = object.get("base64").or_else(|| object.get("data")) {
        let data = data
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{node_name}: base64 image data must be a string"))?;
        base64_admission.admit(data)?;
        return Ok(ImageInput::Base64(data.to_owned()));
    }

    Ok(ImageInput::Path(
        crate::util::node_config::resolve_path_value(value, ctx, node_name)?,
    ))
}

fn validate_source_count(count: usize, maximum: u64) -> Result<()> {
    let count = u64::try_from(count).unwrap_or(u64::MAX);
    if count > maximum {
        anyhow::bail!(
            "image_to_pdf: {count} sources exceed IRONFLOW_MAX_IMAGE_TO_PDF_SOURCES ({maximum})"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_ambiguous_source_objects() {
        let ctx = Context::new();
        let mut base64 = Base64Admission::new(1024, 1024, false);
        for value in [
            serde_json::json!({"path": "x.png", "base64": "AA=="}),
            serde_json::json!({"base64": "AA==", "data": "AA=="}),
            serde_json::json!({"artifact": {}, "artifact_uri": "artifact://sha256/x"}),
        ] {
            let error = parse_image_input(&value, &ctx, "image_to_pdf", &mut base64)
                .unwrap_err()
                .to_string();
            assert!(error.contains("ambiguous"), "{error}");
        }
    }

    #[test]
    fn source_count_is_rejected_before_entries_are_parsed() {
        let ctx = Context::new();
        let config = serde_json::json!({"sources": [false, false]});
        let mut limits = ImageToPdfLimits::current();
        limits.max_sources = 1;
        let error = resolve_image_sources(&config, &ctx, limits)
            .unwrap_err()
            .to_string();
        assert!(error.contains("IRONFLOW_MAX_IMAGE_TO_PDF_SOURCES"));
    }

    #[test]
    fn rejects_invalid_present_config_types() {
        let ctx = Context::new();
        let error =
            resolve_single_image_source(&serde_json::json!({"path": false}), &ctx, "image_resize")
                .unwrap_err()
                .to_string();
        assert!(error.contains("'path' must be a string"));
    }

    #[test]
    fn base64_is_admitted_before_cloning_and_cumulatively_bounded() {
        let ctx = Context::new();
        let mut limits = ImageToPdfLimits::current();
        limits.decode.max_encoded_bytes = 4;
        limits.max_encoded_bytes = 1;
        let config = serde_json::json!({
            "sources": [
                {"base64": "AA=="},
                {"base64": "AA=="}
            ]
        });

        let error = resolve_image_sources(&config, &ctx, limits)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("IRONFLOW_MAX_IMAGE_TO_PDF_ENCODED_BYTES"),
            "{error}"
        );

        let mut admission = Base64Admission::new(1, 1, false);
        let oversized = serde_json::json!({"base64": "AAAA"});
        let error = parse_image_input(&oversized, &ctx, "image_resize", &mut admission)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("IRONFLOW_MAX_IMAGE_ENCODED_BYTES"),
            "{error}"
        );
    }
}
