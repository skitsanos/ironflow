use anyhow::Result;

use crate::engine::types::Context;

mod capped_reader;
mod load;
mod pages;

pub(crate) use load::{decode_image_bytes, inspect_image, load_image, load_image_for_pdf};
pub(crate) use pages::parse_pages_spec;

pub(crate) fn resolve_path(
    config: &serde_json::Value,
    ctx: &Context,
    node_name: &str,
) -> Result<String> {
    crate::util::node_config::get_path(config, ctx, node_name)
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

pub(crate) fn parse_rotation_angle(angle: u64, field: &str) -> Result<u16> {
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

pub(crate) fn image_format_name(format: image::ImageFormat) -> &'static str {
    if format == image::ImageFormat::Jpeg {
        "jpeg"
    } else {
        "png"
    }
}

pub(crate) fn open_pdf_file_capped(
    path: &str,
    node_name: &str,
    execution: &crate::util::execution::ExecutionControl,
) -> Result<capped_reader::CappedFile> {
    let max_bytes = crate::util::limits::max_pdf_bytes();
    capped_reader::CappedFile::open(
        std::path::Path::new(path),
        max_bytes,
        node_name,
        "IRONFLOW_MAX_PDF_BYTES",
        execution,
    )
}
