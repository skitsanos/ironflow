use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::node_config::config_u64;

use super::super::common::{
    load_image_bytes, parse_positive_u32, resolve_image_output_format, save_dynamic_image,
    target_size,
};
use super::super::image_sources::resolve_single_image_source;

pub(crate) struct ImageResizeNode;

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

        let width = config_u64(config, "width", ctx).map(|v| parse_positive_u32(v, "width"));
        let height = config_u64(config, "height", ctx).map(|v| parse_positive_u32(v, "height"));
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
