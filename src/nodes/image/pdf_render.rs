use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::common::{
    load_pdfium, parse_pages_spec, parse_positive_u32, read_pdf_bytes_capped, resolve_image_format,
    validate_pdf_dpi, validate_pdf_render_page_count,
};

pub(crate) struct PdfToImageNode;
pub(crate) struct PdfThumbnailNode;

pub(crate) struct RenderedImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: &'static str,
    pub(crate) base64: String,
}

pub(crate) struct PdfRenderRequest {
    pub(crate) page_count: usize,
    pub(crate) page_idx: usize,
    pub(crate) format: image::ImageFormat,
    pub(crate) width_hint: Option<u32>,
    pub(crate) height_hint: Option<u32>,
    pub(crate) max_side: Option<u32>,
    pub(crate) dpi: f32,
}

pub(crate) fn render_pdf_page(
    document: &pdfium_render::prelude::PdfDocument,
    request: PdfRenderRequest,
) -> Result<RenderedImage> {
    if request.page_idx >= request.page_count {
        anyhow::bail!(
            "page {} exceeds document page count ({})",
            request.page_idx + 1,
            request.page_count
        );
    }

    let pdf_page_idx = i32::try_from(request.page_idx)
        .map_err(|_| anyhow::anyhow!("page index {} is too large", request.page_idx + 1))?;

    let page = document
        .pages()
        .get(pdf_page_idx)
        .map_err(|e| anyhow::anyhow!("Failed to get page {}: {:?}", request.page_idx + 1, e))?;

    let page_width = (page.width().to_inches() * request.dpi).max(1.0);
    let page_height = (page.height().to_inches() * request.dpi).max(1.0);

    let (target_width, target_height) =
        match (request.width_hint, request.height_hint, request.max_side) {
            (Some(w), Some(h), _) => (w, h),
            (Some(w), None, _) => {
                let h = ((page_height * (w as f32 / page_width)).round() as u32).max(1);
                (w, h)
            }
            (None, Some(h), _) => {
                let w = ((page_width * (h as f32 / page_height)).round() as u32).max(1);
                (w, h)
            }
            (None, None, Some(limit)) => {
                if page_width >= page_height {
                    let width = limit;
                    let height = ((page_height / page_width) * width as f32).round() as u32;
                    (width, height.max(1))
                } else {
                    let height = limit;
                    let width = ((page_width / page_height) * height as f32).round() as u32;
                    (width.max(1), height)
                }
            }
            (None, None, None) => (page_width as u32, page_height as u32),
        };

    let pixels = u64::from(target_width).saturating_mul(u64::from(target_height));
    let max_pixels = crate::util::limits::max_pdf_render_pixels();
    if pixels > max_pixels {
        anyhow::bail!(
            "PDF render target {}x{} ({} pixels) exceeds limit {} (set IRONFLOW_MAX_PDF_RENDER_PIXELS to raise)",
            target_width,
            target_height,
            pixels,
            max_pixels
        );
    }

    let render_config = pdfium_render::prelude::PdfRenderConfig::new()
        .set_target_width(target_width as i32)
        .set_target_height(target_height as i32);

    let bitmap = page
        .render_with_config(&render_config)
        .map_err(|e| anyhow::anyhow!("Failed to render page {}: {:?}", request.page_idx + 1, e))?;

    let img = bitmap.as_image().map_err(|e| {
        anyhow::anyhow!(
            "Failed to convert page {} to image: {:?}",
            request.page_idx + 1,
            e
        )
    })?;

    let mut buf: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);

    match request.format {
        image::ImageFormat::Jpeg => {
            img.into_rgb8()
                .write_to(&mut cursor, image::ImageFormat::Jpeg)?;
        }
        _ => {
            img.write_to(&mut cursor, image::ImageFormat::Png)?;
        }
    }

    let base64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    let format_name = if request.format == image::ImageFormat::Jpeg {
        "jpeg"
    } else {
        "png"
    };

    Ok(RenderedImage {
        width: target_width,
        height: target_height,
        format: format_name,
        base64,
    })
}

#[async_trait]
impl Node for PdfToImageNode {
    fn node_type(&self) -> &str {
        "pdf_to_image"
    }

    fn description(&self) -> &str {
        "Render PDF pages to images (requires pdfium library)"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = super::common::resolve_path(config, ctx, "pdf_to_image")?;
        let format = resolve_image_format(
            config.get("format").and_then(|v| v.as_str()),
            "pdf_to_image",
        )?;
        let pages_spec = config
            .get("pages")
            .and_then(|v| v.as_str())
            .unwrap_or("all");
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("images");
        let dpi = config.get("dpi").and_then(|v| v.as_f64()).unwrap_or(150.0) as f32;
        validate_pdf_dpi(dpi, "pdf_to_image")?;

        let bytes = read_pdf_bytes_capped(&path, "pdf_to_image")?;
        let bindings = load_pdfium()?;
        let pdfium = pdfium_render::prelude::Pdfium::new(bindings);
        let document = pdfium
            .load_pdf_from_byte_vec(bytes, None)
            .map_err(|e| anyhow::anyhow!("Failed to open PDF '{}': {:?}", path, e))?;

        let page_count = document.pages().len() as usize;
        let page_indices = parse_pages_spec(pages_spec, page_count)?;
        validate_pdf_render_page_count(page_indices.len(), "pdf_to_image")?;

        let mut images = Vec::new();

        for page_idx in &page_indices {
            let rendered = render_pdf_page(
                &document,
                PdfRenderRequest {
                    page_count,
                    page_idx: *page_idx,
                    format,
                    width_hint: None,
                    height_hint: None,
                    max_side: None,
                    dpi,
                },
            )?;

            images.push(serde_json::json!({
                "page": page_idx + 1,
                "width": rendered.width,
                "height": rendered.height,
                "format": rendered.format,
                "image_base64": rendered.base64,
            }));
        }

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(images));
        output.insert("page_count".to_string(), serde_json::json!(page_count));
        Ok(output)
    }
}

#[async_trait]
impl Node for PdfThumbnailNode {
    fn node_type(&self) -> &str {
        "pdf_thumbnail"
    }

    fn description(&self) -> &str {
        "Render a single PDF page as a thumbnail image (requires pdfium library)"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = super::common::resolve_path(config, ctx, "pdf_thumbnail")?;
        let page = config.get("page").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        if page == 0 {
            anyhow::bail!("pdf_thumbnail: 'page' must be 1-based and >= 1");
        }

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("thumbnail");

        let format = resolve_image_format(
            config.get("format").and_then(|v| v.as_str()),
            "pdf_thumbnail",
        )?;
        let dpi = config.get("dpi").and_then(|v| v.as_f64()).unwrap_or(150.0) as f32;
        validate_pdf_dpi(dpi, "pdf_thumbnail")?;
        let width = config
            .get("width")
            .and_then(|v| v.as_u64())
            .map(|v| parse_positive_u32(v, "width"));
        let height = config
            .get("height")
            .and_then(|v| v.as_u64())
            .map(|v| parse_positive_u32(v, "height"));
        let width = width.transpose()?;
        let height = height.transpose()?;
        let max_side = config.get("size").and_then(|v| v.as_u64()).unwrap_or(256);
        let max_side = parse_positive_u32(max_side, "size")?;

        let bytes = read_pdf_bytes_capped(&path, "pdf_thumbnail")?;
        let bindings = load_pdfium()?;
        let pdfium = pdfium_render::prelude::Pdfium::new(bindings);
        let document = pdfium
            .load_pdf_from_byte_vec(bytes, None)
            .map_err(|e| anyhow::anyhow!("Failed to open PDF '{}': {:?}", path, e))?;
        let page_count = document.pages().len() as usize;

        let rendered = render_pdf_page(
            &document,
            PdfRenderRequest {
                page_count,
                page_idx: page - 1,
                format,
                width_hint: width,
                height_hint: height,
                max_side: Some(max_side),
                dpi,
            },
        )?;

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::json!({
                "page": page,
                "width": rendered.width,
                "height": rendered.height,
                "format": rendered.format,
                "image_base64": rendered.base64,
            }),
        );
        output.insert(
            format!("{}_count", output_key),
            serde_json::value::Value::Number(serde_json::Number::from(1)),
        );
        Ok(output)
    }
}
