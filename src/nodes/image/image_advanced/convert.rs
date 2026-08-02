use std::io::BufWriter;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;
use crate::util::node_config::config_u64_strict;

use super::super::common::{load_image, resolve_image_output_format};
use super::super::image_sources::resolve_single_image_source;
use super::super::resource::{ImageDecodeLimits, validate_combined_allocation};

pub(crate) struct ImageConvertNode;

#[async_trait]
impl Node for ImageConvertNode {
    fn node_type(&self) -> &str {
        "image_convert"
    }

    fn description(&self) -> &str {
        "Convert between image formats"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, ctx, "image_convert")?;
        let output_path = config
            .get("output_path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_convert requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|value| value.as_str())
            .unwrap_or("image_convert")
            .to_owned();
        let quality = config_u64_strict(config, "quality", ctx)?.unwrap_or(85);
        if !(1..=100).contains(&quality) {
            anyhow::bail!("image_convert: 'quality' must be between 1 and 100");
        }
        let quality = u8::try_from(quality).expect("quality range was validated");
        let format = resolve_image_output_format(None, &output_path, "image_convert")?;
        let output_format = std::path::Path::new(&output_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let limits = ImageDecodeLimits::current();

        run_tracked_blocking_step(move |execution| {
            let image = load_image(source, limits, &execution)?.image;
            execution.checkpoint()?;
            if format == image::ImageFormat::Jpeg {
                let source_bytes = u64::try_from(image.as_bytes().len()).unwrap_or(u64::MAX);
                let rgb_bytes = u64::from(image.width())
                    .checked_mul(u64::from(image.height()))
                    .and_then(|pixels| pixels.checked_mul(3))
                    .unwrap_or(u64::MAX);
                validate_combined_allocation("image_convert", source_bytes, rgb_bytes, limits)?;
                let file = std::fs::File::create(&output_path).map_err(|error| {
                    anyhow::anyhow!("image_convert: failed to create '{output_path}': {error}")
                })?;
                let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                    BufWriter::new(file),
                    quality,
                );
                image
                    .to_rgb8()
                    .write_with_encoder(encoder)
                    .map_err(|error| {
                        anyhow::anyhow!("image_convert: failed to encode JPEG: {error}")
                    })?;
            } else {
                image
                    .save_with_format(&output_path, format)
                    .map_err(|error| {
                        anyhow::anyhow!("image_convert: failed to save '{output_path}': {error}")
                    })?;
            }
            execution.checkpoint()?;
            let mut output = NodeOutput::new();
            output.insert(format!("{output_key}_path"), serde_json::json!(output_path));
            output.insert(
                format!("{output_key}_format"),
                serde_json::json!(output_format),
            );
            output.insert(format!("{output_key}_success"), serde_json::json!(true));
            Ok(output)
        })
        .await
    }
}
