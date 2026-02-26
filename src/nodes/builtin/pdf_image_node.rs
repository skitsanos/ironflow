use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct PdfToImageNode;

#[async_trait]
impl Node for PdfToImageNode {
    fn node_type(&self) -> &str {
        "pdf_to_image"
    }

    fn description(&self) -> &str {
        "Render PDF pages to images (requires pdfium library)"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = get_path(config, &ctx)?;
        let pages_spec = config
            .get("pages")
            .and_then(|v| v.as_str())
            .unwrap_or("all");
        let format = config
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("png");
        let dpi = config
            .get("dpi")
            .and_then(|v| v.as_f64())
            .unwrap_or(150.0) as f32;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("images");

        match format {
            "png" | "jpeg" | "jpg" => {}
            other => anyhow::bail!(
                "pdf_to_image: unsupported format '{}'. Must be 'png', 'jpeg', or 'jpg'.",
                other
            ),
        }

        let image_format = match format {
            "jpeg" | "jpg" => image::ImageFormat::Jpeg,
            _ => image::ImageFormat::Png,
        };

        // Initialize pdfium — try PDFIUM_LIB_PATH env, then CWD, then system library
        use pdfium_render::prelude::*;

        let bindings = if let Ok(env_path) = std::env::var("PDFIUM_LIB_PATH") {
            Pdfium::bind_to_library(env_path)
                .map_err(|e| anyhow::anyhow!(
                    "Failed to load pdfium from PDFIUM_LIB_PATH: {:?}", e
                ))?
        } else {
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
                .or_else(|_| Pdfium::bind_to_system_library())
                .map_err(|e| anyhow::anyhow!(
                    "Failed to load pdfium library. Place libpdfium in the working directory or set PDFIUM_LIB_PATH. Error: {:?}",
                    e
                ))?
        };

        let pdfium = Pdfium::new(bindings);

        let bytes = std::fs::read(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;

        let document = pdfium
            .load_pdf_from_byte_vec(bytes, None)
            .map_err(|e| anyhow::anyhow!("Failed to open PDF '{}': {:?}", path, e))?;

        let page_count = document.pages().len() as usize;

        // Parse page specification
        let page_indices = parse_pages_spec(pages_spec, page_count)?;

        let mut images = Vec::new();

        for page_idx in &page_indices {
            let page = document
                .pages()
                .get(*page_idx as u16)
                .map_err(|e| anyhow::anyhow!("Failed to get page {}: {:?}", page_idx + 1, e))?;

            let px_w = (page.width().to_inches() * dpi) as i32;
            let px_h = (page.height().to_inches() * dpi) as i32;

            let render_config = PdfRenderConfig::new()
                .set_target_width(px_w)
                .set_target_height(px_h);

            let bitmap = page
                .render_with_config(&render_config)
                .map_err(|e| anyhow::anyhow!("Failed to render page {}: {:?}", page_idx + 1, e))?;

            let img = bitmap.as_image();

            let mut buf: Vec<u8> = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut buf);

            match image_format {
                image::ImageFormat::Jpeg => {
                    img.into_rgb8()
                        .write_to(&mut cursor, image::ImageFormat::Jpeg)
                        .map_err(|e| anyhow::anyhow!("Failed to encode JPEG: {}", e))?;
                }
                _ => {
                    img.write_to(&mut cursor, image::ImageFormat::Png)
                        .map_err(|e| anyhow::anyhow!("Failed to encode PNG: {}", e))?;
                }
            }

            let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);

            images.push(serde_json::json!({
                "page": page_idx + 1,
                "width": px_w,
                "height": px_h,
                "format": format,
                "image_base64": b64
            }));
        }

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(images));
        output.insert(
            "page_count".to_string(),
            serde_json::json!(page_count),
        );
        Ok(output)
    }
}

/// Parse a page specification string into 0-based page indices.
/// Supports: "all", "1", "1,3,5", "1-5", "1-3,7,9-11"
fn parse_pages_spec(spec: &str, page_count: usize) -> Result<Vec<usize>> {
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
                anyhow::bail!(
                    "Page {} exceeds document page count ({})",
                    end,
                    page_count
                );
            }

            for i in start..=end {
                indices.push(i - 1); // convert to 0-based
            }
        } else {
            let page: usize = part
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid page number: '{}'", part))?;

            if page == 0 {
                anyhow::bail!("Page numbers are 1-based, got 0");
            }
            if page > page_count {
                anyhow::bail!(
                    "Page {} exceeds document page count ({})",
                    page,
                    page_count
                );
            }

            indices.push(page - 1);
        }
    }

    if indices.is_empty() {
        anyhow::bail!("No pages specified");
    }

    Ok(indices)
}

fn get_path(config: &serde_json::Value, ctx: &Context) -> Result<String> {
    let has_path = config.get("path").and_then(|v| v.as_str()).is_some();
    let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

    if has_path && has_source_key {
        anyhow::bail!("pdf_to_image accepts either 'path' or 'source_key', not both");
    }

    if let Some(path_str) = config.get("path").and_then(|v| v.as_str()) {
        Ok(interpolate_ctx(path_str, ctx))
    } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
        let val = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        match val {
            serde_json::Value::String(s) => Ok(s.clone()),
            _ => anyhow::bail!("Context key '{}' must be a string (file path)", source_key),
        }
    } else {
        anyhow::bail!("pdf_to_image requires either 'path' or 'source_key'")
    }
}
