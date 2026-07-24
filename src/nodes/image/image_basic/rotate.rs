use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::node_config::config_u64;

use super::super::common::{
    image_format_name, load_image_bytes, parse_rotation_angle, resolve_image_output_format,
    save_dynamic_image,
};
use super::super::image_sources::resolve_single_image_source;

pub(crate) struct ImageRotateNode;

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
            .map(|_| {
                config_u64(config, "angle", ctx)
                    .ok_or_else(|| anyhow::anyhow!("angle: must be one of 90, 180, or 270"))
                    .and_then(|value| parse_rotation_angle(value, "angle"))
            })
            .transpose()?
            .unwrap_or(90);

        let source_image = load_image_bytes(source)?;
        let source_width = source_image.image.width();
        let source_height = source_image.image.height();
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
        for (suffix, value) in [
            ("angle", u32::from(angle)),
            ("width", rotated.width()),
            ("height", rotated.height()),
            ("source_width", source_width),
            ("source_height", source_height),
        ] {
            output.insert(
                format!("{}_{}", output_key, suffix),
                serde_json::Value::Number(serde_json::Number::from(u64::from(value))),
            );
        }
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
