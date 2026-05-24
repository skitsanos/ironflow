use anyhow::Result;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

pub(crate) fn resolve_path(
    config: &serde_json::Value,
    ctx: &Context,
    node_name: &str,
) -> Result<String> {
    let has_path = config.get("path").and_then(|v| v.as_str()).is_some();
    let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

    if has_path && has_source_key {
        anyhow::bail!(
            "{} accepts either 'path' or 'source_key', not both",
            node_name
        );
    }

    if let Some(path_str) = config.get("path").and_then(|v| v.as_str()) {
        Ok(interpolate_ctx(path_str, ctx))
    } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
        let val = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        match val {
            serde_json::Value::String(s) => Ok(s.clone()),
            _ => {
                anyhow::bail!("Context key '{}' must be a string (file path)", source_key)
            }
        }
    } else {
        anyhow::bail!("{} requires either 'path' or 'source_key'", node_name)
    }
}

pub(crate) fn resolve_image_format(
    format: Option<&str>,
    node_name: &str,
) -> Result<image::ImageFormat> {
    match format.unwrap_or("png") {
        "png" => Ok(image::ImageFormat::Png),
        "jpeg" | "jpg" => Ok(image::ImageFormat::Jpeg),
        other => anyhow::bail!(
            "{}: unsupported format '{}'. Must be 'png', 'jpeg', or 'jpg'.",
            node_name,
            other
        ),
    }
}

pub(crate) fn resolve_image_output_format(
    format: Option<&str>,
    output_path: &str,
    node_name: &str,
) -> Result<image::ImageFormat> {
    if let Some(format) = format {
        return resolve_image_format(Some(format), node_name);
    }

    let extension = std::path::Path::new(output_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase);

    match extension.as_deref() {
        Some("jpg") | Some("jpeg") => Ok(image::ImageFormat::Jpeg),
        Some("png") => Ok(image::ImageFormat::Png),
        Some(other) => anyhow::bail!(
            "{}: unsupported output extension '.{}'. Supported: png, jpg, jpeg",
            node_name,
            other
        ),
        None => Ok(image::ImageFormat::Png),
    }
}

pub(crate) fn load_image_bytes(
    input: super::image_sources::ImageInput,
) -> Result<super::image_sources::LoadedImage> {
    use base64::Engine;
    let (label, bytes) = match input {
        super::image_sources::ImageInput::Path(path) => {
            let bytes = std::fs::read(&path).map_err(|e| {
                anyhow::anyhow!("image_to_pdf: failed to read image '{}': {}", path, e)
            })?;
            (path, bytes)
        }
        super::image_sources::ImageInput::Base64(data) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| {
                    anyhow::anyhow!("image_to_pdf: failed to decode base64 image data: {}", e)
                })?;
            ("base64_image".to_string(), bytes)
        }
    };

    let image = image::load_from_memory(&bytes)
        .map_err(|e| anyhow::anyhow!("image_to_pdf: invalid image data for '{}': {}", label, e))?;
    Ok(super::image_sources::LoadedImage {
        label,
        bytes,
        image,
    })
}

pub(crate) fn save_dynamic_image(
    image: image::DynamicImage,
    output_path: &str,
    format: image::ImageFormat,
) -> Result<()> {
    image
        .save_with_format(output_path, format)
        .map_err(|e| anyhow::anyhow!("{}", e))
}

pub(crate) fn load_pdfium() -> Result<Box<dyn pdfium_render::prelude::PdfiumLibraryBindings>> {
    use pdfium_render::prelude::*;
    if let Ok(env_path) = std::env::var("PDFIUM_LIB_PATH") {
        Pdfium::bind_to_library(env_path)
            .map_err(|e| anyhow::anyhow!("Failed to load pdfium from PDFIUM_LIB_PATH: {:?}", e))
    } else {
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
            .or_else(|_| Pdfium::bind_to_system_library())
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to load pdfium library. Place libpdfium in the working directory or set PDFIUM_LIB_PATH. Error: {:?}",
                    e
                )
            })
    }
}

pub(crate) fn parse_positive_u32(value: u64, field: &str) -> Result<u32> {
    let n = u32::try_from(value).map_err(|_| {
        anyhow::anyhow!("{}: value {} is too large (max {})", field, value, u32::MAX)
    })?;
    if n == 0 {
        anyhow::bail!("{}: must be >= 1", field);
    }
    Ok(n)
}

pub(crate) fn parse_non_negative_u32(value: u64, field: &str) -> Result<u32> {
    u32::try_from(value)
        .map_err(|_| anyhow::anyhow!("{}: value {} is too large (max {})", field, value, u32::MAX))
}

pub(crate) fn parse_rotation_angle(value: &serde_json::Value, field: &str) -> Result<u16> {
    let angle = value
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("{}: must be one of 90, 180, or 270", field))?;
    match angle {
        90 | 180 | 270 => Ok(angle as u16),
        0 => Ok(90),
        _ => anyhow::bail!(
            "{}: unsupported angle '{}'. Supported values: 90, 180, 270",
            field,
            angle
        ),
    }
}

pub(crate) fn validate_pdf_dpi(dpi: f32, node_name: &str) -> Result<()> {
    if !dpi.is_finite() || dpi <= 0.0 {
        anyhow::bail!("{}: dpi must be a positive finite number", node_name);
    }

    let max_dpi = crate::util::limits::max_pdf_dpi() as f32;
    if dpi > max_dpi {
        anyhow::bail!(
            "{}: dpi {} exceeds limit {} (set IRONFLOW_MAX_PDF_DPI to raise)",
            node_name,
            dpi,
            max_dpi
        );
    }
    Ok(())
}

pub(crate) fn validate_pdf_render_page_count(page_count: usize, node_name: &str) -> Result<()> {
    let max_pages = crate::util::limits::max_pdf_render_pages() as usize;
    if page_count > max_pages {
        anyhow::bail!(
            "{}: requested {} rendered pages, exceeds limit {} (set IRONFLOW_MAX_PDF_RENDER_PAGES to raise)",
            node_name,
            page_count,
            max_pages
        );
    }
    Ok(())
}

pub(crate) fn target_size(
    source_width: u32,
    source_height: u32,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<(u32, u32)> {
    let (target_w, target_h) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => {
            let h = ((w as f32) * (source_height as f32) / (source_width as f32)).round();
            (w, h.max(1.0) as u32)
        }
        (None, Some(h)) => {
            let w = ((h as f32) * (source_width as f32) / (source_height as f32)).round();
            (w.max(1.0) as u32, h)
        }
        _ => anyhow::bail!("target_size requires either width or height"),
    };

    Ok((target_w.max(1), target_h.max(1)))
}

/// Parse a page specification string into 0-based page indices.
/// Supports: "all", "1", "1,3,5", "1-5", "1-3,7,9-11"
pub(crate) fn parse_pages_spec(spec: &str, page_count: usize) -> Result<Vec<usize>> {
    if spec == "all" {
        return Ok((0..page_count).collect());
    }

    let mut indices = Vec::new();

    for part in spec.split(',') {
        let part = part.trim();
        if let Some((start_s, end_s)) = part.split_once('-') {
            let start: usize = start_s
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid page number: '{}'", start_s.trim()))?;
            let end: usize = end_s
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid page number: '{}'", end_s.trim()))?;

            if start == 0 || end == 0 {
                anyhow::bail!("Page numbers are 1-based, got 0");
            }
            if start > end {
                anyhow::bail!("Invalid page range: {}-{}", start, end);
            }
            if end > page_count {
                anyhow::bail!("Page {} exceeds document page count ({})", end, page_count);
            }

            for i in start..=end {
                indices.push(i - 1);
            }
        } else {
            let page: usize = part
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid page number: '{}'", part))?;

            if page == 0 {
                anyhow::bail!("Page numbers are 1-based, got 0");
            }
            if page > page_count {
                anyhow::bail!("Page {} exceeds document page count ({})", page, page_count);
            }

            indices.push(page - 1);
        }
    }

    if indices.is_empty() {
        anyhow::bail!("No pages specified");
    }

    Ok(indices)
}

pub(crate) fn image_format_name(format: image::ImageFormat) -> &'static str {
    if format == image::ImageFormat::Jpeg {
        "jpeg"
    } else {
        "png"
    }
}

pub(crate) fn read_pdf_bytes_capped(path: &str, node_name: &str) -> Result<Vec<u8>> {
    let max_bytes = crate::util::limits::max_pdf_bytes();
    let meta = std::fs::metadata(path)
        .map_err(|e| anyhow::anyhow!("{}: failed to stat '{}': {}", node_name, path, e))?;
    if meta.len() > max_bytes {
        anyhow::bail!(
            "{}: PDF '{}' is {} bytes, exceeds limit {} (set IRONFLOW_MAX_PDF_BYTES to raise)",
            node_name,
            path,
            meta.len(),
            max_bytes
        );
    }

    std::fs::read(path).map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))
}
