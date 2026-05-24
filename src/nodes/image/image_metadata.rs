use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

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
        let path = super::common::resolve_path(config, ctx, "image_metadata")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("image_metadata");

        let (width, height) = image::image_dimensions(&path)
            .map_err(|e| anyhow::anyhow!("image_metadata: failed to read '{}': {}", path, e))?;

        let format = std::path::Path::new(&path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
            .unwrap_or_default();

        let img = image::open(&path)
            .map_err(|e| anyhow::anyhow!("image_metadata: failed to open '{}': {}", path, e))?;
        let color_type = format!("{:?}", img.color());

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(width))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(height))),
        );
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(format),
        );
        output.insert(
            format!("{}_color_type", output_key),
            serde_json::Value::String(color_type),
        );
        Ok(output)
    }
}
