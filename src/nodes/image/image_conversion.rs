use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;

use super::image_sources::{ImageInput, resolve_image_sources};
use super::resource::ImageToPdfLimits;

mod pdf_image;
mod worker;

pub(crate) struct ImageToPdfNode;

pub(super) struct Request {
    sources: Vec<ImageInput>,
    output_key: String,
    output_path: String,
    limits: ImageToPdfLimits,
}

#[async_trait]
impl Node for ImageToPdfNode {
    fn node_type(&self) -> &str {
        "image_to_pdf"
    }

    fn description(&self) -> &str {
        "Convert one or more images to a PDF file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let limits = ImageToPdfLimits::current();
        let sources = resolve_image_sources(config, ctx, limits)?;
        if sources.is_empty() {
            anyhow::bail!("image_to_pdf requires at least one image in 'sources'");
        }
        limits.validate_source_count(sources.len())?;
        let output_key = config
            .get("output_key")
            .and_then(|value| value.as_str())
            .unwrap_or("pdf_path")
            .to_owned();
        let output_path = config
            .get("output_path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_to_pdf requires 'output_path' parameter"))?;
        let request = Request {
            sources,
            output_key,
            output_path: interpolate_ctx(output_path, ctx),
            limits,
        };
        run_tracked_blocking_step(move |execution| worker::convert(request, &execution)).await
    }
}
