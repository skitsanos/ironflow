use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;
use crate::util::node_config::config_u64;

use super::super::common::{
    load_image, parse_positive_u32, resolve_image_output_format, save_dynamic_image, target_size,
};
use super::super::image_sources::resolve_single_image_source;
use super::super::resource::{ImageDecodeLimits, validate_output_shape};

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
            .unwrap_or("resized_image")
            .to_owned();
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

        let limits = ImageDecodeLimits::current();
        run_tracked_blocking_step(move |execution| {
            let source_loaded = load_image(source, limits, &execution)?;
            let source_bytes =
                u64::try_from(source_loaded.image.as_bytes().len()).unwrap_or(u64::MAX);
            let (target_w, target_h) = target_size(
                source_loaded.image.width(),
                source_loaded.image.height(),
                width,
                height,
            )?;
            validate_output_shape(
                "image_resize",
                target_w,
                target_h,
                source_loaded.image.color(),
                source_bytes,
                limits,
            )?;
            execution.checkpoint()?;
            let resized = source_loaded.image.resize_exact(
                target_w,
                target_h,
                image::imageops::FilterType::Lanczos3,
            );
            execution.checkpoint()?;
            save_dynamic_image(resized, &output_path, format)?;
            execution.checkpoint()?;

            let mut output = NodeOutput::new();
            output.insert(output_key.clone(), serde_json::json!(output_path));
            output.insert(format!("{output_key}_width"), serde_json::json!(target_w));
            output.insert(format!("{output_key}_height"), serde_json::json!(target_h));
            output.insert(
                format!("{output_key}_format"),
                serde_json::json!(super::super::common::image_format_name(format)),
            );
            output.insert(format!("{output_key}_success"), serde_json::json!(true));
            Ok(output)
        })
        .await
    }
}
