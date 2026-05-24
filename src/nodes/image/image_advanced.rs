use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::common::{
    image_format_name, load_image_bytes, resolve_image_output_format, save_dynamic_image,
};
use super::image_sources::resolve_single_image_source;

pub(crate) struct ImageGrayscaleNode;
pub(crate) struct ImageConvertNode;
pub(crate) struct ImageWatermarkNode;

#[async_trait]
impl Node for ImageGrayscaleNode {
    fn node_type(&self) -> &str {
        "image_grayscale"
    }

    fn description(&self) -> &str {
        "Convert a single image to grayscale"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, ctx, "image_grayscale")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_grayscale requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("grayscale_image");
        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_grayscale",
        )?;

        let image = load_image_bytes(source)?.image.grayscale();

        save_dynamic_image(image.clone(), &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::Value::String(output_path),
        );
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(image.width()))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(image.height()))),
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
impl Node for ImageConvertNode {
    fn node_type(&self) -> &str {
        "image_convert"
    }

    fn description(&self) -> &str {
        "Convert between image formats"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = super::common::resolve_path(config, ctx, "image_convert")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_convert requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("image_convert");
        let quality = config.get("quality").and_then(|v| v.as_u64()).unwrap_or(85) as u8;

        let img = image::open(&path)
            .map_err(|e| anyhow::anyhow!("image_convert: failed to open '{}': {}", path, e))?;

        let format = resolve_image_output_format(None, &output_path, "image_convert")?;

        if format == image::ImageFormat::Jpeg {
            let rgb = img.to_rgb8();
            let mut buf: Vec<u8> = Vec::new();
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
            rgb.write_with_encoder(encoder)
                .map_err(|e| anyhow::anyhow!("image_convert: failed to encode JPEG: {}", e))?;
            std::fs::write(&output_path, buf).map_err(|e| {
                anyhow::anyhow!("image_convert: failed to write '{}': {}", output_path, e)
            })?;
        } else {
            img.save(&output_path).map_err(|e| {
                anyhow::anyhow!("image_convert: failed to save '{}': {}", output_path, e)
            })?;
        }

        let out_format = std::path::Path::new(&output_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
            .unwrap_or_default();

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_path", output_key),
            serde_json::Value::String(output_path),
        );
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(out_format),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

#[async_trait]
impl Node for ImageWatermarkNode {
    fn node_type(&self) -> &str {
        "image_watermark"
    }

    fn description(&self) -> &str {
        "Overlay a semi-transparent watermark band on an image"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, ctx, "image_watermark")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_watermark requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("image_watermark");
        let text = config
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("watermark");
        let text = interpolate_ctx(text, ctx);
        let position = config
            .get("position")
            .and_then(|v| v.as_str())
            .unwrap_or("bottom-right");
        let opacity = config
            .get("opacity")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0) as f32;

        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_watermark",
        )?;

        let mut img = load_image_bytes(source)?.image.to_rgba8();
        let (img_w, img_h) = (img.width(), img.height());

        // Band dimensions: width proportional to text length, height ~5% of image
        let band_h = (img_h as f32 * 0.05).max(10.0) as u32;
        let band_w = ((text.len() as f32) * (band_h as f32) * 0.6)
            .min(img_w as f32)
            .max(band_h as f32) as u32;

        let alpha = (opacity * 255.0) as u8;

        // Calculate band position
        let (band_x, band_y) = match position {
            "top-left" => (0u32, 0u32),
            "top-right" => (img_w.saturating_sub(band_w), 0),
            "bottom-left" => (0, img_h.saturating_sub(band_h)),
            "center" => (
                img_w.saturating_sub(band_w) / 2,
                img_h.saturating_sub(band_h) / 2,
            ),
            _ => (
                // bottom-right (default)
                img_w.saturating_sub(band_w),
                img_h.saturating_sub(band_h),
            ),
        };

        // Draw semi-transparent dark band
        for y in band_y..band_y.saturating_add(band_h).min(img_h) {
            for x in band_x..band_x.saturating_add(band_w).min(img_w) {
                let pixel = img.get_pixel_mut(x, y);
                // Blend with dark overlay
                let bg_r = pixel[0] as f32;
                let bg_g = pixel[1] as f32;
                let bg_b = pixel[2] as f32;
                let a = alpha as f32 / 255.0;
                pixel[0] = (bg_r * (1.0 - a)) as u8;
                pixel[1] = (bg_g * (1.0 - a)) as u8;
                pixel[2] = (bg_b * (1.0 - a)) as u8;
            }
        }

        let dynamic = image::DynamicImage::ImageRgba8(img);
        save_dynamic_image(dynamic, &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_path", output_key),
            serde_json::Value::String(output_path),
        );
        output.insert(
            format!("{}_text", output_key),
            serde_json::Value::String(text),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}
