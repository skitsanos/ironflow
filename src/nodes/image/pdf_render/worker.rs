use anyhow::Result;

use super::{PagesRequest, ThumbnailRequest};
use crate::artifacts::{ArtifactRef, LocalArtifactStore};
use crate::engine::types::NodeOutput;
use crate::util::execution::ExecutionControl;

use crate::nodes::image::common::{load_pdfium, open_pdf_file_capped, parse_pages_spec};

struct RenderRequest {
    page_count: usize,
    page_index: usize,
    format: image::ImageFormat,
    width_hint: Option<u32>,
    height_hint: Option<u32>,
    max_side: Option<u32>,
    dpi: f32,
}

struct RenderedImage {
    width: u32,
    height: u32,
    format: &'static str,
    artifact: ArtifactRef,
}

pub(super) fn render_pages(
    request: PagesRequest,
    execution: &ExecutionControl,
) -> Result<NodeOutput> {
    execution.checkpoint()?;
    let file = open_pdf_file_capped(&request.source, "pdf_to_image", execution)?;
    let bindings = load_pdfium()?;
    let pdfium = pdfium_render::prelude::Pdfium::new(bindings);
    let document = pdfium
        .load_pdf_from_reader(file, None)
        .map_err(|error| anyhow::anyhow!("Failed to open verified PDF input: {error:?}"))?;
    let page_count = document.pages().len() as usize;
    let page_indices = parse_pages_spec(
        &request.pages,
        page_count,
        crate::util::limits::max_pdf_render_pages(),
        "pdf_to_image",
        "IRONFLOW_MAX_PDF_RENDER_PAGES",
    )?;
    let store = LocalArtifactStore::from_env()?;
    let mut images = Vec::new();
    images.try_reserve_exact(page_indices.len())?;
    for page_index in page_indices {
        execution.checkpoint()?;
        let rendered = render_pdf_page(
            &document,
            RenderRequest {
                page_count,
                page_index,
                format: request.format,
                width_hint: None,
                height_hint: None,
                max_side: None,
                dpi: request.dpi,
            },
            &store,
            execution,
        )?;
        images.push(image_json(page_index + 1, rendered)?);
    }
    Ok(NodeOutput::from([
        (request.output_key, serde_json::Value::Array(images)),
        ("page_count".to_owned(), serde_json::json!(page_count)),
    ]))
}

pub(super) fn render_thumbnail(
    request: ThumbnailRequest,
    execution: &ExecutionControl,
) -> Result<NodeOutput> {
    execution.checkpoint()?;
    let file = open_pdf_file_capped(&request.source, "pdf_thumbnail", execution)?;
    let bindings = load_pdfium()?;
    let pdfium = pdfium_render::prelude::Pdfium::new(bindings);
    let document = pdfium
        .load_pdf_from_reader(file, None)
        .map_err(|error| anyhow::anyhow!("Failed to open verified PDF input: {error:?}"))?;
    let page_count = document.pages().len() as usize;
    let store = LocalArtifactStore::from_env()?;
    let rendered = render_pdf_page(
        &document,
        RenderRequest {
            page_count,
            page_index: request.page - 1,
            format: request.format,
            width_hint: request.width,
            height_hint: request.height,
            max_side: Some(request.max_side),
            dpi: request.dpi,
        },
        &store,
        execution,
    )?;
    let key = request.output_key;
    Ok(NodeOutput::from([
        (key.clone(), image_json(request.page, rendered)?),
        (format!("{key}_count"), serde_json::json!(1)),
    ]))
}

fn render_pdf_page(
    document: &pdfium_render::prelude::PdfDocument<'_>,
    request: RenderRequest,
    store: &LocalArtifactStore,
    execution: &ExecutionControl,
) -> Result<RenderedImage> {
    if request.page_index >= request.page_count {
        anyhow::bail!(
            "page {} exceeds document page count ({})",
            request.page_index + 1,
            request.page_count
        );
    }
    execution.checkpoint()?;
    let page_index = i32::try_from(request.page_index)
        .map_err(|_| anyhow::anyhow!("page index {} is too large", request.page_index + 1))?;
    let page = document.pages().get(page_index).map_err(|error| {
        anyhow::anyhow!("Failed to get page {}: {error:?}", request.page_index + 1)
    })?;
    let page_width = (page.width().to_inches() * request.dpi).max(1.0);
    let page_height = (page.height().to_inches() * request.dpi).max(1.0);
    let (width, height) = dimensions(page_width, page_height, &request);
    validate_pixels(width, height)?;
    let config = pdfium_render::prelude::PdfRenderConfig::new()
        .set_target_width(width as i32)
        .set_target_height(height as i32);
    let bitmap = page.render_with_config(&config).map_err(|error| {
        anyhow::anyhow!(
            "Failed to render page {}: {error:?}",
            request.page_index + 1
        )
    })?;
    let image = bitmap.as_image().map_err(|error| {
        anyhow::anyhow!(
            "Failed to convert page {} to image: {error:?}",
            request.page_index + 1
        )
    })?;
    execution.checkpoint()?;
    let (format, mime_type) = if request.format == image::ImageFormat::Jpeg {
        ("jpeg", "image/jpeg")
    } else {
        ("png", "image/png")
    };
    let artifact = store.put_writer(
        crate::util::limits::max_file_bytes(),
        Some(mime_type.to_owned()),
        execution,
        move |file| {
            if request.format == image::ImageFormat::Jpeg {
                image.into_rgb8().write_to(file, image::ImageFormat::Jpeg)?;
            } else {
                image.write_to(file, image::ImageFormat::Png)?;
            }
            Ok(())
        },
    )?;
    Ok(RenderedImage {
        width,
        height,
        format,
        artifact,
    })
}

fn dimensions(page_width: f32, page_height: f32, request: &RenderRequest) -> (u32, u32) {
    match (request.width_hint, request.height_hint, request.max_side) {
        (Some(width), Some(height), _) => (width, height),
        (Some(width), None, _) => {
            let height = ((page_height * (width as f32 / page_width)).round() as u32).max(1);
            (width, height)
        }
        (None, Some(height), _) => {
            let width = ((page_width * (height as f32 / page_height)).round() as u32).max(1);
            (width, height)
        }
        (None, None, Some(limit)) if page_width >= page_height => {
            let height = ((page_height / page_width) * limit as f32).round() as u32;
            (limit, height.max(1))
        }
        (None, None, Some(limit)) => {
            let width = ((page_width / page_height) * limit as f32).round() as u32;
            (width.max(1), limit)
        }
        (None, None, None) => (page_width as u32, page_height as u32),
    }
}

fn validate_pixels(width: u32, height: u32) -> Result<()> {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    let limit = crate::util::limits::max_pdf_render_pixels();
    if pixels > limit {
        anyhow::bail!(
            "PDF render target {width}x{height} ({pixels} pixels) exceeds limit {limit} \
             (set IRONFLOW_MAX_PDF_RENDER_PIXELS to raise)"
        );
    }
    Ok(())
}

fn image_json(page: usize, rendered: RenderedImage) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "page": page,
        "width": rendered.width,
        "height": rendered.height,
        "format": rendered.format,
        "artifact": serde_json::to_value(rendered.artifact)?,
    }))
}
