use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;

use super::super::common::{
    image_format_name, load_image, resolve_image_output_format, save_dynamic_image,
};
use super::super::image_sources::resolve_single_image_source;
use super::super::resource::{ImageDecodeLimits, validate_combined_allocation};

pub(crate) struct ImageGrayscaleNode;

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
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_grayscale requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|value| value.as_str())
            .unwrap_or("grayscale_image")
            .to_owned();
        let format = resolve_image_output_format(
            config.get("format").and_then(|value| value.as_str()),
            &output_path,
            "image_grayscale",
        )?;
        let limits = ImageDecodeLimits::current();

        run_tracked_blocking_step(move |execution| {
            let source = load_image(source, limits, &execution)?.image;
            let source_bytes = u64::try_from(source.as_bytes().len()).unwrap_or(u64::MAX);
            let output_bytes = u64::from(source.width())
                .checked_mul(u64::from(source.height()))
                .and_then(|pixels| pixels.checked_mul(grayscale_bytes_per_pixel(source.color())))
                .unwrap_or(u64::MAX);
            validate_combined_allocation("image_grayscale", source_bytes, output_bytes, limits)?;
            let image = source.grayscale();
            execution.checkpoint()?;
            let width = image.width();
            let height = image.height();
            save_dynamic_image(image, &output_path, format)?;
            execution.checkpoint()?;

            let mut output = NodeOutput::new();
            output.insert(output_key.clone(), serde_json::json!(output_path));
            output.insert(format!("{output_key}_width"), serde_json::json!(width));
            output.insert(format!("{output_key}_height"), serde_json::json!(height));
            output.insert(
                format!("{output_key}_format"),
                serde_json::json!(image_format_name(format)),
            );
            output.insert(format!("{output_key}_success"), serde_json::json!(true));
            Ok(output)
        })
        .await
    }
}

fn grayscale_bytes_per_pixel(color: image::ColorType) -> u64 {
    match color {
        image::ColorType::L16
        | image::ColorType::La16
        | image::ColorType::Rgb16
        | image::ColorType::Rgba16 => 2,
        image::ColorType::Rgb32F | image::ColorType::Rgba32F => 4,
        _ => 1,
    }
}
