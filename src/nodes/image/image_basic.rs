use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::common::{
    image_format_name, load_image_bytes, parse_non_negative_u32, parse_positive_u32,
    parse_rotation_angle, resolve_image_output_format, save_dynamic_image, target_size,
};
use super::image_sources::resolve_single_image_source;

pub(crate) struct ImageResizeNode;
pub(crate) struct ImageCropNode;
pub(crate) struct ImageRotateNode;
pub(crate) struct ImageFlipNode;

#[async_trait]
impl Node for ImageResizeNode {
    fn node_type(&self) -> &str {
        "image_resize"
    }

    fn description(&self) -> &str {
        "Resize a single image"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, ctx, "image_resize")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_resize requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("resized_image");
        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_resize",
        )?;

        let width = config
            .get("width")
            .and_then(|v| v.as_u64())
            .map(|v| parse_positive_u32(v, "width"));
        let height = config
            .get("height")
            .and_then(|v| v.as_u64())
            .map(|v| parse_positive_u32(v, "height"));
        let width = width.transpose()?;
        let height = height.transpose()?;

        if width.is_none() && height.is_none() {
            anyhow::bail!("image_resize requires either 'width' or 'height'");
        }

        let source_loaded = load_image_bytes(source)?;
        let (target_w, target_h) = target_size(
            source_loaded.image.width(),
            source_loaded.image.height(),
            width,
            height,
        )?;

        let resized = source_loaded.image.resize_exact(
            target_w,
            target_h,
            image::imageops::FilterType::Lanczos3,
        );

        save_dynamic_image(resized, &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::Value::String(output_path),
        );
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(target_w))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(target_h))),
        );
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

        let x = parse_non_negative_u32(config.get("x").and_then(|v| v.as_u64()).unwrap_or(0), "x")?;
        let y = parse_non_negative_u32(config.get("y").and_then(|v| v.as_u64()).unwrap_or(0), "y")?;

        let (crop_w, crop_w_field) = if let Some(width_val) = config.get("crop_width") {
            (
                width_val.as_u64().ok_or_else(|| {
                    anyhow::anyhow!("image_crop: 'crop_width' must be a positive number")
                })?,
                "crop_width",
            )
        } else {
            (
                config
                    .get("width")
                    .ok_or_else(|| anyhow::anyhow!("image_crop requires 'crop_width' or 'width'"))?
                    .as_u64()
                    .ok_or_else(|| {
                        anyhow::anyhow!("image_crop: 'width' must be a positive number")
                    })?,
                "width",
            )
        };
        let (crop_h, crop_h_field) = if let Some(height_val) = config.get("crop_height") {
            (
                height_val.as_u64().ok_or_else(|| {
                    anyhow::anyhow!("image_crop: 'crop_height' must be a positive number")
                })?,
                "crop_height",
            )
        } else {
            (
                config
                    .get("height")
                    .ok_or_else(|| {
                        anyhow::anyhow!("image_crop requires 'crop_height' or 'height'")
                    })?
                    .as_u64()
                    .ok_or_else(|| {
                        anyhow::anyhow!("image_crop: 'height' must be a positive number")
                    })?,
                "height",
            )
        };

        let crop_w = parse_positive_u32(crop_w, crop_w_field)?;
        let crop_h = parse_positive_u32(crop_h, crop_h_field)?;

        let source_loaded = load_image_bytes(source)?;

        if x >= source_loaded.image.width() || y >= source_loaded.image.height() {
            anyhow::bail!(
                "image_crop: starting point ({}, {}) is outside image bounds ({}x{})",
                x,
                y,
                source_loaded.image.width(),
                source_loaded.image.height()
            );
        }

        let crop_right = x.checked_add(crop_w).ok_or_else(|| {
            anyhow::anyhow!("image_crop: crop start + width overflows image width")
        })?;
        let crop_bottom = y.checked_add(crop_h).ok_or_else(|| {
            anyhow::anyhow!("image_crop: crop start + height overflows image height")
        })?;

        if crop_right > source_loaded.image.width() {
            anyhow::bail!(
                "image_crop: crop width {} exceeds image bounds at x={} (image width {})",
                crop_w,
                x,
                source_loaded.image.width()
            );
        }
        if crop_bottom > source_loaded.image.height() {
            anyhow::bail!(
                "image_crop: crop height {} exceeds image bounds at y={} (image height {})",
                crop_h,
                y,
                source_loaded.image.height()
            );
        }

        let cropped = source_loaded.image.crop_imm(x, y, crop_w, crop_h);
        save_dynamic_image(cropped, &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::Value::String(output_path),
        );
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(crop_w))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(crop_h))),
        );
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(if format == image::ImageFormat::Jpeg {
                "jpeg".to_string()
            } else {
                "png".to_string()
            }),
        );
        output.insert(
            format!("{}_x", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(x))),
        );
        output.insert(
            format!("{}_y", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(y))),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

#[async_trait]
impl Node for ImageRotateNode {
    fn node_type(&self) -> &str {
        "image_rotate"
    }

    fn description(&self) -> &str {
        "Rotate a single image by 90-degree increments"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, ctx, "image_rotate")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_rotate requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("rotated_image");
        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_rotate",
        )?;
        let angle = config
            .get("angle")
            .map(|value| parse_rotation_angle(value, "angle"))
            .transpose()?
            .unwrap_or(90);

        let source_image = load_image_bytes(source)?;
        let width = source_image.image.width();
        let height = source_image.image.height();

        let rotated = match angle {
            90 => source_image.image.rotate90(),
            180 => source_image.image.rotate180(),
            270 => source_image.image.rotate270(),
            _ => unreachable!("invalid rotation angle already validated"),
        };

        save_dynamic_image(rotated.clone(), &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::Value::String(output_path),
        );
        output.insert(
            format!("{}_angle", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(angle))),
        );
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(rotated.width()))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(rotated.height()))),
        );
        output.insert(
            format!("{}_source_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(width))),
        );
        output.insert(
            format!("{}_source_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(height))),
        );
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(image_format_name(format).to_string()),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

#[async_trait]
impl Node for ImageFlipNode {
    fn node_type(&self) -> &str {
        "image_flip"
    }

    fn description(&self) -> &str {
        "Flip a single image horizontally or vertically"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, ctx, "image_flip")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_flip requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("flipped_image");
        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_flip",
        )?;
        let direction = config
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("horizontal")
            .to_lowercase();

        let source_image = load_image_bytes(source)?;
        let flipped = match direction.as_str() {
            "horizontal" | "h" => source_image.image.fliph(),
            "vertical" | "v" => source_image.image.flipv(),
            "both" => source_image.image.flipv().fliph(),
            _ => {
                anyhow::bail!(
                    "image_flip: unsupported direction '{}'. Use 'horizontal', 'vertical', or 'both'",
                    direction
                );
            }
        };

        save_dynamic_image(flipped.clone(), &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::Value::String(output_path),
        );
        output.insert(
            format!("{}_direction", output_key),
            serde_json::Value::String(direction),
        );
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(flipped.width()))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(flipped.height()))),
        );
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(image_format_name(format).to_string()),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}
