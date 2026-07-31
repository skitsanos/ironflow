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
use super::super::resource::{ImageDecodeLimits, validate_output_shape};

pub(crate) struct ImageFlipNode;

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
            .unwrap_or("flipped_image")
            .to_owned();
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

        let limits = ImageDecodeLimits::current();
        run_tracked_blocking_step(move |execution| {
            let source_image = load_image(source, limits, &execution)?;
            let source_bytes = u64::try_from(source_image.image.as_bytes().len())
                .unwrap_or(u64::MAX);
            validate_output_shape(
                "image_flip",
                source_image.image.width(),
                source_image.image.height(),
                source_image.image.color(),
                source_bytes,
                limits,
            )?;
            execution.checkpoint()?;
            let flipped = match direction.as_str() {
                "horizontal" | "h" => source_image.image.fliph(),
                "vertical" | "v" => source_image.image.flipv(),
                "both" => source_image.image.flipv().fliph(),
                _ => anyhow::bail!(
                    "image_flip: unsupported direction '{}'. Use 'horizontal', 'vertical', or 'both'",
                    direction
                ),
            };
            execution.checkpoint()?;
            let width = flipped.width();
            let height = flipped.height();
            save_dynamic_image(flipped, &output_path, format)?;
            execution.checkpoint()?;

            let mut output = NodeOutput::new();
            output.insert(output_key.clone(), serde_json::json!(output_path));
            output.insert(format!("{output_key}_direction"), serde_json::json!(direction));
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
