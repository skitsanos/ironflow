mod worker;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;
use crate::util::node_config::{config_f64_or, config_u64_strict, config_usize_strict};

use super::common::{parse_positive_u32, resolve_image_format, validate_pdf_dpi};

pub(crate) struct PdfToImageNode;
pub(crate) struct PdfThumbnailNode;

pub(super) struct PagesRequest {
    path: String,
    pages: String,
    output_key: String,
    format: image::ImageFormat,
    dpi: f32,
}

pub(super) struct ThumbnailRequest {
    path: String,
    page: usize,
    output_key: String,
    format: image::ImageFormat,
    dpi: f32,
    width: Option<u32>,
    height: Option<u32>,
    max_side: u32,
}

#[async_trait]
impl Node for PdfToImageNode {
    fn node_type(&self) -> &str {
        "pdf_to_image"
    }

    fn description(&self) -> &str {
        "Render PDF pages to disk-backed image artifacts (requires pdfium library)"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = super::common::resolve_path(config, ctx, "pdf_to_image")?;
        let format = resolve_image_format(optional_string(config, "format")?, "pdf_to_image")?;
        let pages = optional_string(config, "pages")?
            .unwrap_or("all")
            .to_owned();
        let output_key = optional_string(config, "output_key")?
            .unwrap_or("images")
            .to_owned();
        let dpi = config_f64_or(config, "dpi", ctx, 150.0)? as f32;
        validate_pdf_dpi(dpi, "pdf_to_image")?;
        let request = PagesRequest {
            path,
            pages,
            output_key,
            format,
            dpi,
        };
        run_tracked_blocking_step(move |execution| worker::render_pages(request, &execution)).await
    }
}

#[async_trait]
impl Node for PdfThumbnailNode {
    fn node_type(&self) -> &str {
        "pdf_thumbnail"
    }

    fn description(&self) -> &str {
        "Render one PDF page to a disk-backed image artifact (requires pdfium library)"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = super::common::resolve_path(config, ctx, "pdf_thumbnail")?;
        let page = config_usize_strict(config, "page", ctx)?.unwrap_or(1);
        if page == 0 {
            anyhow::bail!("pdf_thumbnail: 'page' must be 1-based and >= 1");
        }
        let output_key = optional_string(config, "output_key")?
            .unwrap_or("thumbnail")
            .to_owned();
        let format = resolve_image_format(optional_string(config, "format")?, "pdf_thumbnail")?;
        let dpi = config_f64_or(config, "dpi", ctx, 150.0)? as f32;
        validate_pdf_dpi(dpi, "pdf_thumbnail")?;
        let width = positive_optional(config, "width", ctx)?;
        let height = positive_optional(config, "height", ctx)?;
        let max_side = config_u64_strict(config, "size", ctx)?.unwrap_or(256);
        let max_side = parse_positive_u32(max_side, "size")?;
        let request = ThumbnailRequest {
            path,
            page,
            output_key,
            format,
            dpi,
            width,
            height,
            max_side,
        };
        run_tracked_blocking_step(move |execution| worker::render_thumbnail(request, &execution))
            .await
    }
}

fn positive_optional(config: &serde_json::Value, key: &str, ctx: &Context) -> Result<Option<u32>> {
    config_u64_strict(config, key, ctx)?
        .map(|value| parse_positive_u32(value, key))
        .transpose()
}

fn optional_string<'a>(config: &'a serde_json::Value, key: &str) -> Result<Option<&'a str>> {
    match config.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(_) => anyhow::bail!("'{key}' must be a string"),
    }
}
