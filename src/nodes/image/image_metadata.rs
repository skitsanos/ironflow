use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;

use super::common::inspect_image;
use super::image_sources::resolve_single_image_source;
use super::resource::ImageDecodeLimits;

pub(crate) struct ImageMetadataNode;

#[async_trait]
impl Node for ImageMetadataNode {
    fn node_type(&self) -> &str {
        "image_metadata"
    }

    fn description(&self) -> &str {
        "Extract metadata from an image file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, ctx, "image_metadata")?;
        let output_key = config
            .get("output_key")
            .and_then(|value| value.as_str())
            .unwrap_or("image_metadata")
            .to_owned();
        let limits = ImageDecodeLimits::current();

        run_tracked_blocking_step(move |execution| {
            let info = inspect_image(source, limits, &execution).map_err(|error| {
                anyhow::anyhow!("image_metadata: failed to inspect image: {error}")
            })?;
            let mut output = NodeOutput::new();
            output.insert(format!("{output_key}_width"), serde_json::json!(info.width));
            output.insert(
                format!("{output_key}_height"),
                serde_json::json!(info.height),
            );
            output.insert(
                format!("{output_key}_format"),
                serde_json::json!(format!("{:?}", info.format).to_ascii_lowercase()),
            );
            output.insert(
                format!("{output_key}_color_type"),
                serde_json::json!(format!("{:?}", info.color_type)),
            );
            Ok(output)
        })
        .await
    }
}
