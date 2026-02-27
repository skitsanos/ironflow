use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, xobject, Document, Object, Stream};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct PdfToImageNode;
pub struct PdfThumbnailNode;
pub struct ImageToPdfNode;
pub struct PdfMetadataNode;
pub struct ImageResizeNode;
pub struct ImageCropNode;
pub struct ImageRotateNode;
pub struct ImageFlipNode;
pub struct ImageGrayscaleNode;

#[async_trait]
impl Node for PdfToImageNode {
    fn node_type(&self) -> &str {
        "pdf_to_image"
    }

    fn description(&self) -> &str {
        "Render PDF pages to images (requires pdfium library)"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = resolve_path(config, &ctx, "pdf_to_image")?;
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

        let bytes = std::fs::read(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;
        let bindings = load_pdfium()?;
        let pdfium = pdfium_render::prelude::Pdfium::new(bindings);
        let document = pdfium
            .load_pdf_from_byte_vec(bytes.clone(), None)
            .map_err(|e| anyhow::anyhow!("Failed to open PDF '{}': {:?}", path, e))?;

        let page_count = document.pages().len() as usize;
        let page_indices = parse_pages_spec(pages_spec, page_count)?;

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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = resolve_path(config, &ctx, "pdf_thumbnail")?;
        let page = config
            .get("page")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;
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
        let max_side = config
            .get("size")
            .and_then(|v| v.as_u64())
            .unwrap_or(256);
        let max_side = parse_positive_u32(max_side, "size")?;

        let bytes = std::fs::read(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;
        let bindings = load_pdfium()?;
        let pdfium = pdfium_render::prelude::Pdfium::new(bindings);
        let document = pdfium
            .load_pdf_from_byte_vec(bytes.clone(), None)
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

#[async_trait]
impl Node for ImageToPdfNode {
    fn node_type(&self) -> &str {
        "image_to_pdf"
    }

    fn description(&self) -> &str {
        "Convert one or more images to a PDF file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let sources = resolve_image_sources(config, &ctx)?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("pdf_path");
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_to_pdf requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, &ctx);

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut page_ids = Vec::new();

        if sources.is_empty() {
            anyhow::bail!("image_to_pdf requires at least one image in 'sources'");
        }

        for source in sources {
            let loaded = load_image_bytes(source)?;
            if loaded.image.width() == 0 || loaded.image.height() == 0 {
                anyhow::bail!("image_to_pdf: image dimensions must be > 0");
            }

            let image_stream = xobject::image_from(loaded.bytes).map_err(|e| {
                anyhow::anyhow!("image_to_pdf: failed to parse image '{}': {:?}", loaded.label, e)
            })?;
            let image_id = doc.add_object(image_stream);
            let image_name = format!("X{}", image_id.0);
            let width = loaded.image.width();
            let height = loaded.image.height();

            let media_box = vec![
                0.into(),
                0.into(),
                i64::from(width).into(),
                i64::from(height).into(),
            ];
            let mut content = Content {
                operations: vec![],
            };
            content
                .operations
                .push(Operation::new("q", vec![]));
            content.operations.push(Operation::new(
                "cm",
                vec![
                    width.into(),
                    0.into(),
                    0.into(),
                    height.into(),
                    0.into(),
                    0.into(),
                ],
            ));
            content
                .operations
                .push(Operation::new("Do", vec![Object::Name(image_name.clone().into_bytes())]));
            content.operations.push(Operation::new("Q", vec![]));

            let content_id = doc.add_object(Stream::new(
                dictionary! {},
                content
                    .encode()
                    .map_err(|e| anyhow::anyhow!("image_to_pdf failed to encode content stream: {:?}", e))?,
            ));

            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "MediaBox" => media_box,
            });

            doc.add_xobject(page_id, image_name.as_bytes(), image_id)
                .map_err(|e| anyhow::anyhow!("image_to_pdf failed to add image resource: {:?}", e))?;
            page_ids.push(page_id);
        }

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|id| lopdf::Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => page_ids.len() as u32,
        };
        doc.objects.insert(pages_id, lopdf::Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });

        doc.trailer.set("Root", catalog_id);
        doc.compress();
        doc.save(&output_path)
            .map_err(|e| anyhow::anyhow!("image_to_pdf: failed to save PDF '{}': {:?}", output_path, e))?;

        let mut out = NodeOutput::new();
        out.insert(output_key.to_string(), serde_json::json!(output_path));
        out.insert("image_count".to_string(), serde_json::json!(page_ids.len()));
        out.insert(format!("{}_count", output_key), serde_json::json!(page_ids.len()));
        out.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(out)
    }
}

#[async_trait]
impl Node for ImageResizeNode {
    fn node_type(&self) -> &str {
        "image_resize"
    }

    fn description(&self) -> &str {
        "Resize a single image"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, &ctx, "image_resize")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_resize requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, &ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("resized_image");
        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_resize",
        )?;

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

        if width.is_none() && height.is_none() {
            anyhow::bail!("image_resize requires either 'width' or 'height'");
        }

        let source_loaded = load_image_bytes(source)?;
        let (target_w, target_h) = target_size(
            source_loaded.image.width(),
            source_loaded.image.height(),
            width,
            height,
        )?;

        let resized = source_loaded
            .image
            .resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3);

        save_dynamic_image(
            resized,
            &output_path,
            format,
        )?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(output_path));
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(target_w))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(target_h))),
        );
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(if format == image::ImageFormat::Jpeg {
                "jpeg".to_string()
            } else {
                "png".to_string()
            }),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

#[async_trait]
impl Node for ImageCropNode {
    fn node_type(&self) -> &str {
        "image_crop"
    }

    fn description(&self) -> &str {
        "Crop a single image"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, &ctx, "image_crop")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_crop requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, &ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("cropped_image");
        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_crop",
        )?;

        let x = parse_non_negative_u32(config.get("x").and_then(|v| v.as_u64()).unwrap_or(0), "x")?;
        let y = parse_non_negative_u32(config.get("y").and_then(|v| v.as_u64()).unwrap_or(0), "y")?;

        let (crop_w, crop_w_field) = if let Some(width_val) = config.get("crop_width") {
            (
                width_val
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("image_crop: 'crop_width' must be a positive number"))?,
                "crop_width",
            )
        } else {
            (
                config
                    .get("width")
                    .ok_or_else(|| anyhow::anyhow!("image_crop requires 'crop_width' or 'width'"))?
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("image_crop: 'width' must be a positive number"))?,
                "width",
            )
        };
        let (crop_h, crop_h_field) = if let Some(height_val) = config.get("crop_height") {
            (
                height_val
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("image_crop: 'crop_height' must be a positive number"))?,
                "crop_height",
            )
        } else {
            (
                config
                    .get("height")
                    .ok_or_else(|| anyhow::anyhow!("image_crop requires 'crop_height' or 'height'"))?
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("image_crop: 'height' must be a positive number"))?,
                "height",
            )
        };

        let crop_w = parse_positive_u32(crop_w, crop_w_field)?;
        let crop_h = parse_positive_u32(crop_h, crop_h_field)?;

        let source_loaded = load_image_bytes(source)?;

        if x >= source_loaded.image.width() || y >= source_loaded.image.height() {
            anyhow::bail!(
                "image_crop: starting point ({}, {}) is outside image bounds ({}x{})",
                x,
                y,
                source_loaded.image.width(),
                source_loaded.image.height()
            );
        }

        let crop_right = x
            .checked_add(crop_w)
            .ok_or_else(|| anyhow::anyhow!("image_crop: crop start + width overflows image width"))?;
        let crop_bottom = y
            .checked_add(crop_h)
            .ok_or_else(|| anyhow::anyhow!("image_crop: crop start + height overflows image height"))?;

        if crop_right > source_loaded.image.width() {
            anyhow::bail!(
                "image_crop: crop width {} exceeds image bounds at x={} (image width {})",
                crop_w,
                x,
                source_loaded.image.width()
            );
        }
        if crop_bottom > source_loaded.image.height() {
            anyhow::bail!(
                "image_crop: crop height {} exceeds image bounds at y={} (image height {})",
                crop_h,
                y,
                source_loaded.image.height()
            );
        }

        let cropped = source_loaded.image.crop_imm(x, y, crop_w, crop_h);
        save_dynamic_image(cropped, &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(output_path));
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(crop_w))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(crop_h))),
        );
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(if format == image::ImageFormat::Jpeg {
                "jpeg".to_string()
            } else {
                "png".to_string()
            }),
        );
        output.insert(
            format!("{}_x", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(x))),
        );
        output.insert(
            format!("{}_y", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(y))),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

#[async_trait]
impl Node for PdfMetadataNode {
    fn node_type(&self) -> &str {
        "pdf_metadata"
    }

    fn description(&self) -> &str {
        "Extract PDF metadata and page count"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = resolve_path(config, &ctx, "pdf_metadata")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("metadata");

        let bytes = std::fs::read(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;
        let metadata = extract_pdf_metadata_for_node(&bytes)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::to_value(metadata)?);
        Ok(output)
    }
}

#[async_trait]
impl Node for ImageRotateNode {
    fn node_type(&self) -> &str {
        "image_rotate"
    }

    fn description(&self) -> &str {
        "Rotate a single image by 90-degree increments"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, &ctx, "image_rotate")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_rotate requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, &ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("rotated_image");
        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_rotate",
        )?;
        let angle = config
            .get("angle")
            .map(|value| parse_rotation_angle(value, "angle"))
            .transpose()?
            .unwrap_or(90);

        let source_image = load_image_bytes(source)?;
        let width = source_image.image.width();
        let height = source_image.image.height();

        let rotated = match angle {
            90 => source_image.image.rotate90(),
            180 => source_image.image.rotate180(),
            270 => source_image.image.rotate270(),
            _ => unreachable!("invalid rotation angle already validated"),
        };

        save_dynamic_image(rotated.clone(), &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(output_path));
        output.insert(
            format!("{}_angle", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(angle))),
        );
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(rotated.width()))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(rotated.height()))),
        );
        output.insert(
            format!("{}_source_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(width))),
        );
        output.insert(
            format!("{}_source_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(height))),
        );
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(image_format_name(format).to_string()),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

#[async_trait]
impl Node for ImageFlipNode {
    fn node_type(&self) -> &str {
        "image_flip"
    }

    fn description(&self) -> &str {
        "Flip a single image horizontally or vertically"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, &ctx, "image_flip")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_flip requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, &ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("flipped_image");
        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_flip",
        )?;
        let direction = config
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("horizontal")
            .to_lowercase();

        let source_image = load_image_bytes(source)?;
        let flipped = match direction.as_str() {
            "horizontal" | "h" => source_image.image.fliph(),
            "vertical" | "v" => source_image.image.flipv(),
            "both" => source_image.image.flipv().fliph(),
            _ => {
                anyhow::bail!(
                    "image_flip: unsupported direction '{}'. Use 'horizontal', 'vertical', or 'both'",
                    direction
                );
            }
        };

        save_dynamic_image(flipped.clone(), &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(output_path));
        output.insert(
            format!("{}_direction", output_key),
            serde_json::Value::String(direction),
        );
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(flipped.width()))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(flipped.height()))),
        );
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(image_format_name(format).to_string()),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

#[async_trait]
impl Node for ImageGrayscaleNode {
    fn node_type(&self) -> &str {
        "image_grayscale"
    }

    fn description(&self) -> &str {
        "Convert a single image to grayscale"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source = resolve_single_image_source(config, &ctx, "image_grayscale")?;
        let output_path = config
            .get("output_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("image_grayscale requires 'output_path' parameter"))?;
        let output_path = interpolate_ctx(output_path, &ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("grayscale_image");
        let format = resolve_image_output_format(
            config.get("format").and_then(|v| v.as_str()),
            &output_path,
            "image_grayscale",
        )?;

        let image = load_image_bytes(source)?.image.grayscale();

        save_dynamic_image(image.clone(), &output_path, format)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(output_path));
        output.insert(
            format!("{}_width", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(image.width()))),
        );
        output.insert(
            format!("{}_height", output_key),
            serde_json::Value::Number(serde_json::Number::from(u64::from(image.height()))),
        );
        output.insert(
            format!("{}_format", output_key),
            serde_json::Value::String(image_format_name(format).to_string()),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

struct RenderedImage {
    width: u32,
    height: u32,
    format: &'static str,
    base64: String,
}

#[derive(Debug)]
enum ImageInput {
    Path(String),
    Base64(String),
}

#[derive(Debug)]
struct LoadedImage {
    label: String,
    bytes: Vec<u8>,
    image: image::DynamicImage,
}

fn image_format_name(format: image::ImageFormat) -> &'static str {
    if format == image::ImageFormat::Jpeg {
        "jpeg"
    } else {
        "png"
    }
}

fn parse_rotation_angle(value: &serde_json::Value, field: &str) -> Result<u16> {
    let angle = value
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("{}: must be one of 90, 180, or 270", field))?;
    match angle {
        90 | 180 | 270 => Ok(angle as u16),
        0 => Ok(90),
        _ => anyhow::bail!("{}: unsupported angle '{}'. Supported values: 90, 180, 270", field, angle),
    }
}

fn extract_pdf_metadata_for_node(bytes: &[u8]) -> Result<BTreeMap<String, serde_json::Value>> {
    let mut metadata = BTreeMap::new();
    let doc = lopdf::Document::load_mem(bytes)
        .map_err(|e| anyhow::anyhow!("pdf_metadata: failed to parse PDF: {:?}", e))?;

    let page_count = doc.get_pages().len();
    metadata.insert("pages".to_string(), serde_json::json!(page_count));

    if let Ok(info_ref) = doc.trailer.get(b"Info")
        && let Ok(obj_ref) = info_ref.as_reference()
        && let Ok(info_obj) = doc.get_object(obj_ref)
        && let Ok(dict) = info_obj.as_dict()
    {
        let fields = [
            (b"Title".as_slice(), "title"),
            (b"Author".as_slice(), "author"),
            (b"Subject".as_slice(), "subject"),
            (b"Keywords".as_slice(), "keywords"),
            (b"Creator".as_slice(), "creator"),
            (b"Producer".as_slice(), "producer"),
            (b"CreationDate".as_slice(), "created"),
            (b"ModDate".as_slice(), "modified"),
        ];

        for (pdf_key, label) in fields {
            if let Ok(val) = dict.get(pdf_key)
                && let Ok(bytes) = val.as_str()
            {
                let s = String::from_utf8_lossy(bytes).trim().to_string();
                if !s.is_empty() {
                    metadata.insert(label.to_string(), serde_json::Value::String(s));
                }
            }
        }
    }

    Ok(metadata)
}

fn resolve_image_format(format: Option<&str>, node_name: &str) -> Result<image::ImageFormat> {
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

fn resolve_image_output_format(
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

fn parse_positive_u32(value: u64, field: &str) -> Result<u32> {
    let n = u32::try_from(value).map_err(|_| {
        anyhow::anyhow!("{}: value {} is too large (max {})", field, value, u32::MAX)
    })?;
    if n == 0 {
        anyhow::bail!("{}: must be >= 1", field);
    }
    Ok(n)
}

fn parse_non_negative_u32(value: u64, field: &str) -> Result<u32> {
    u32::try_from(value).map_err(|_| {
        anyhow::anyhow!(
            "{}: value {} is too large (max {})",
            field,
            value,
            u32::MAX
        )
    })
}

fn target_size(
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

fn resolve_single_image_source(
    config: &serde_json::Value,
    ctx: &Context,
    node_name: &str,
) -> Result<ImageInput> {
    let has_path = config.get("path").and_then(|v| v.as_str()).is_some();
    let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

    if has_path && has_source_key {
        anyhow::bail!("{} accepts either 'path' or 'source_key', not both", node_name);
    }

    if let Some(path) = config.get("path").and_then(|v| v.as_str()) {
        Ok(ImageInput::Path(interpolate_ctx(path, ctx)))
    } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
        let val = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        parse_image_input(val, ctx)
    } else {
        anyhow::bail!("{} requires either 'path' or 'source_key'", node_name)
    }
}

fn load_pdfium() -> Result<Box<dyn pdfium_render::prelude::PdfiumLibraryBindings>> {
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

struct PdfRenderRequest {
    page_count: usize,
    page_idx: usize,
    format: image::ImageFormat,
    width_hint: Option<u32>,
    height_hint: Option<u32>,
    max_side: Option<u32>,
    dpi: f32,
}

fn render_pdf_page(
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

    let page = document
        .pages()
        .get(request.page_idx as u16)
        .map_err(|e| {
            anyhow::anyhow!("Failed to get page {}: {:?}", request.page_idx + 1, e)
        })?;

    let page_width = (page.width().to_inches() * request.dpi).max(1.0);
    let page_height = (page.height().to_inches() * request.dpi).max(1.0);

    let (target_width, target_height) = match (request.width_hint, request.height_hint, request.max_side) {
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

    let render_config = pdfium_render::prelude::PdfRenderConfig::new()
        .set_target_width(target_width as i32)
        .set_target_height(target_height as i32);

    let bitmap = page
        .render_with_config(&render_config)
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to render page {}: {:?}",
                request.page_idx + 1,
                e
            )
        })?;

    let img = bitmap.as_image();

    let mut buf: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);

    match request.format {
        image::ImageFormat::Jpeg => {
            img.into_rgb8().write_to(&mut cursor, image::ImageFormat::Jpeg)?;
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

fn resolve_path(config: &serde_json::Value, ctx: &Context, node_name: &str) -> Result<String> {
    let has_path = config.get("path").and_then(|v| v.as_str()).is_some();
    let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

    if has_path && has_source_key {
        anyhow::bail!("{} accepts either 'path' or 'source_key', not both", node_name);
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

fn resolve_image_sources(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<Vec<ImageInput>> {
    let has_sources = config.get("sources").is_some();
    let has_source_key = config.get("source_key").is_some();

    if has_sources && has_source_key {
        anyhow::bail!("image_to_pdf accepts either 'sources' or 'source_key', not both");
    }

    let from_config = if let Some(sources) = config.get("sources") {
        sources
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("image_to_pdf: 'sources' must be an array"))?
            .iter()
            .map(|value| parse_image_input(value, ctx))
            .collect::<Result<Vec<_>>>()?
    } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
        let val = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        val.as_array()
            .ok_or_else(|| anyhow::anyhow!("Context key '{}' must be an array", source_key))?
            .iter()
            .map(|value| parse_image_input(value, ctx))
            .collect::<Result<Vec<_>>>()?
    } else {
        anyhow::bail!("image_to_pdf requires either 'sources' or 'source_key'")
    };

    Ok(from_config)
}

fn parse_image_input(value: &serde_json::Value, ctx: &Context) -> Result<ImageInput> {
    if let Some(path) = value.as_str() {
        return Ok(ImageInput::Path(interpolate_ctx(path, ctx)));
    }

    let value = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("image_to_pdf source entries must be strings or objects"))?;

    if let Some(path) = value.get("path").and_then(|v| v.as_str()) {
        Ok(ImageInput::Path(interpolate_ctx(path, ctx)))
    } else if let Some(data) = value.get("base64").and_then(|v| v.as_str()) {
        Ok(ImageInput::Base64(data.to_string()))
    } else if let Some(data) = value.get("data").and_then(|v| v.as_str()) {
        Ok(ImageInput::Base64(data.to_string()))
    } else {
        Err(anyhow::anyhow!(
            "image_to_pdf source object must include 'path' or 'base64'/'data'"
        ))
    }
}

fn load_image_bytes(input: ImageInput) -> Result<LoadedImage> {
    let (label, bytes) = match input {
        ImageInput::Path(path) => {
            let bytes = std::fs::read(&path)
                .map_err(|e| anyhow::anyhow!("image_to_pdf: failed to read image '{}': {}", path, e))?;
            (path, bytes)
        }
        ImageInput::Base64(data) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| anyhow::anyhow!("image_to_pdf: failed to decode base64 image data: {}", e))?;
            ("base64_image".to_string(), bytes)
        }
    };

    let image = image::load_from_memory(&bytes)
        .map_err(|e| anyhow::anyhow!("image_to_pdf: invalid image data for '{}': {}", label, e))?;
    Ok(LoadedImage {
        label,
        bytes,
        image,
    })
}

fn save_dynamic_image(
    image: image::DynamicImage,
    output_path: &str,
    format: image::ImageFormat,
) -> Result<()> {
    image
        .save_with_format(output_path, format)
        .map_err(|e| anyhow::anyhow!("{}", e))
}
