use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::super::common::{
    image_format_name, load_image_bytes, resolve_image_output_format, save_dynamic_image,
};
use super::super::image_sources::resolve_single_image_source;

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
