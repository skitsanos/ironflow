use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::execution::{ExecutionControl, run_tracked_blocking_step};
use crate::util::node_config::config_f64;

use super::super::common::{load_image, resolve_image_output_format, save_dynamic_image};
use super::super::image_sources::resolve_single_image_source;
use super::super::resource::{ImageDecodeLimits, validate_combined_allocation};

pub(crate) struct ImageWatermarkNode;

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
        let output_path = required_string(config, "output_path", "image_watermark")?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|value| value.as_str())
            .unwrap_or("image_watermark")
            .to_owned();
        let text = interpolate_ctx(
            config
                .get("text")
                .and_then(|value| value.as_str())
                .unwrap_or("watermark"),
            ctx,
        );
        let position = config
            .get("position")
            .and_then(|value| value.as_str())
            .unwrap_or("bottom-right")
            .to_owned();
        let opacity = config_f64(config, "opacity", ctx)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0) as f32;
        let format = resolve_image_output_format(
            config.get("format").and_then(|value| value.as_str()),
            &output_path,
            "image_watermark",
        )?;
        let limits = ImageDecodeLimits::current();

        run_tracked_blocking_step(move |execution| {
            let source = load_image(source, limits, &execution)?.image;
            let source_bytes = u64::try_from(source.as_bytes().len()).unwrap_or(u64::MAX);
            let rgba_bytes = u64::from(source.width())
                .checked_mul(u64::from(source.height()))
                .and_then(|pixels| pixels.checked_mul(4))
                .unwrap_or(u64::MAX);
            validate_combined_allocation("image_watermark", source_bytes, rgba_bytes, limits)?;
            let mut image = source.to_rgba8();
            draw_band(&mut image, &text, &position, opacity, &execution)?;
            execution.checkpoint()?;
            save_dynamic_image(image::DynamicImage::ImageRgba8(image), &output_path, format)?;
            execution.checkpoint()?;
            let mut output = NodeOutput::new();
            output.insert(format!("{output_key}_path"), serde_json::json!(output_path));
            output.insert(format!("{output_key}_text"), serde_json::json!(text));
            output.insert(format!("{output_key}_success"), serde_json::json!(true));
            Ok(output)
        })
        .await
    }
}

fn draw_band(
    image: &mut image::RgbaImage,
    text: &str,
    position: &str,
    opacity: f32,
    execution: &ExecutionControl,
) -> Result<()> {
    let (width, height) = image.dimensions();
    let band_height = (height as f32 * 0.05).max(10.0) as u32;
    let band_width = ((text.len() as f32) * (band_height as f32) * 0.6)
        .min(width as f32)
        .max(band_height as f32) as u32;
    let (band_x, band_y) = band_origin(width, height, band_width, band_height, position);
    let alpha = opacity * 255.0;
    for y in band_y..band_y.saturating_add(band_height).min(height) {
        if (y - band_y) % 64 == 0 {
            execution.checkpoint()?;
        }
        for x in band_x..band_x.saturating_add(band_width).min(width) {
            let pixel = image.get_pixel_mut(x, y);
            let factor = 1.0 - alpha / 255.0;
            pixel[0] = (pixel[0] as f32 * factor) as u8;
            pixel[1] = (pixel[1] as f32 * factor) as u8;
            pixel[2] = (pixel[2] as f32 * factor) as u8;
        }
    }
    execution.checkpoint()
}

fn band_origin(width: u32, height: u32, band_w: u32, band_h: u32, position: &str) -> (u32, u32) {
    match position {
        "top-left" => (0, 0),
        "top-right" => (width.saturating_sub(band_w), 0),
        "bottom-left" => (0, height.saturating_sub(band_h)),
        "center" => (
            width.saturating_sub(band_w) / 2,
            height.saturating_sub(band_h) / 2,
        ),
        _ => (width.saturating_sub(band_w), height.saturating_sub(band_h)),
    }
}

fn required_string<'a>(
    config: &'a serde_json::Value,
    key: &str,
    node_name: &str,
) -> Result<&'a str> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("{node_name} requires '{key}' parameter"))
}
