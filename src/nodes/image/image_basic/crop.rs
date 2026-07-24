use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::node_config::config_u64;

use super::super::common::{
    load_image_bytes, parse_non_negative_u32, parse_positive_u32, resolve_image_output_format,
    save_dynamic_image,
};
use super::super::image_sources::resolve_single_image_source;

pub(crate) struct ImageCropNode;

#[async_trait]
impl Node for ImageCropNode {
    fn node_type(&self) -> &str {
        "image_crop"
    }

    fn description(&self) -> &str {
        "Crop a single image"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, ctx, "image_crop")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_crop requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("cropped_image");
        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_crop",
        )?;

        let x = parse_non_negative_u32(config_u64(config, "x", ctx).unwrap_or(0), "x")?;
        let y = parse_non_negative_u32(config_u64(config, "y", ctx).unwrap_or(0), "y")?;
        let (crop_w, crop_w_field) = dimension(config, ctx, "crop_width", "width")?;
        let (crop_h, crop_h_field) = dimension(config, ctx, "crop_height", "height")?;
        let crop_w = parse_positive_u32(crop_w, crop_w_field)?;
        let crop_h = parse_positive_u32(crop_h, crop_h_field)?;

        let source_loaded = load_image_bytes(source)?;
        validate_crop_bounds(&source_loaded.image, x, y, crop_w, crop_h)?;
        let cropped = source_loaded.image.crop_imm(x, y, crop_w, crop_h);
        save_dynamic_image(cropped, &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::Value::String(output_path),
        );
        for (suffix, value) in [("width", crop_w), ("height", crop_h), ("x", x), ("y", y)] {
            output.insert(
                format!("{}_{}", output_key, suffix),
                serde_json::Value::Number(serde_json::Number::from(u64::from(value))),
            );
        }
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(if format == image::ImageFormat::Jpeg {
                "jpeg".to_string()
            } else {
                "png".to_string()
            }),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

fn dimension<'a>(
    config: &serde_json::Value,
    ctx: &Context,
    preferred: &'a str,
    fallback: &'a str,
) -> Result<(u64, &'a str)> {
    if config.get(preferred).is_some() {
        let value = config_u64(config, preferred, ctx).ok_or_else(|| {
            anyhow::anyhow!("image_crop: '{}' must be a positive number", preferred)
        })?;
        return Ok((value, preferred));
    }

    config
        .get(fallback)
        .ok_or_else(|| anyhow::anyhow!("image_crop requires '{}' or '{}'", preferred, fallback))?;
    let value = config_u64(config, fallback, ctx)
        .ok_or_else(|| anyhow::anyhow!("image_crop: '{}' must be a positive number", fallback))?;
    Ok((value, fallback))
}

fn validate_crop_bounds(
    image: &image::DynamicImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<()> {
    if x >= image.width() || y >= image.height() {
        anyhow::bail!(
            "image_crop: starting point ({}, {}) is outside image bounds ({}x{})",
            x,
            y,
            image.width(),
            image.height()
        );
    }
    let right = x
        .checked_add(width)
        .ok_or_else(|| anyhow::anyhow!("image_crop: crop start + width overflows image width"))?;
    let bottom = y
        .checked_add(height)
        .ok_or_else(|| anyhow::anyhow!("image_crop: crop start + height overflows image height"))?;
    if right > image.width() {
        anyhow::bail!(
            "image_crop: crop width {} exceeds image bounds at x={} (image width {})",
            width,
            x,
            image.width()
        );
    }
    if bottom > image.height() {
        anyhow::bail!(
            "image_crop: crop height {} exceeds image bounds at y={} (image height {})",
            height,
            y,
            image.height()
        );
    }
    Ok(())
}
