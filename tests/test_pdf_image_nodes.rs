use base64::Engine;
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use tempfile::tempdir;

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

fn is_pdfium_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("pdfium") || msg.contains("failed to load")
}

fn sample_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/samples")
}

fn write_temp_png(path: &std::path::Path, color: [u8; 4], width: u32, height: u32) {
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        width,
        height,
        image::Rgba(color),
    ));
    let mut buf: Vec<u8> = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .unwrap();
    std::fs::write(path, buf).unwrap();
}

#[tokio::test]
async fn pdf_to_image_generates_base64_pages() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("pdf_to_image").unwrap();

    let sample_pdf = sample_root().join("Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf");

    let config = serde_json::json!({
        "path": sample_pdf.to_string_lossy(),
        "pages": "1",
        "output_key": "images"
    });

    let result = match node.execute(&config, empty_ctx()).await {
        Ok(result) => result,
        Err(err) => {
            if is_pdfium_error(&err) {
                return;
            }
            panic!("unexpected error: {}", err);
        }
    };

    let images = result.get("images").unwrap().as_array().unwrap();
    assert_eq!(images.len(), 1);
    assert!(images[0].get("image_base64").is_some());
    assert_eq!(result.get("page_count").unwrap().as_u64().unwrap(), 1);
}

#[tokio::test]
async fn pdf_thumbnail_generates_one_thumbnail() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("pdf_thumbnail").unwrap();

    let sample_pdf = sample_root().join("Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf");

    let config = serde_json::json!({
        "path": sample_pdf.to_string_lossy(),
        "page": 1,
        "size": 128,
        "output_key": "thumb"
    });

    let result = match node.execute(&config, empty_ctx()).await {
        Ok(result) => result,
        Err(err) => {
            if is_pdfium_error(&err) {
                return;
            }
            panic!("unexpected error: {}", err);
        }
    };

    let thumb = result.get("thumb").unwrap().as_object().unwrap();
    assert_eq!(result.get("thumb_count").unwrap().as_u64().unwrap(), 1);
    assert_eq!(thumb.get("page").unwrap(), 1);
    assert!(thumb.get("image_base64").is_some());
}

#[tokio::test]
async fn image_to_pdf_accepts_paths() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_to_pdf").unwrap();
    let dir = tempdir().unwrap();
    let out_dir = dir.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let front = dir.path().join("front.png");
    let back = dir.path().join("back.png");
    write_temp_png(&front, [255, 0, 0, 255], 64, 48);
    write_temp_png(&back, [0, 255, 0, 255], 64, 48);

    let output_path = out_dir.join("combined.pdf");

    let config = serde_json::json!({
        "sources": [
            serde_json::json!({ "path": front.to_string_lossy() }),
            serde_json::json!({ "path": back.to_string_lossy() }),
        ],
        "output_path": output_path.to_string_lossy(),
        "output_key": "pdf_path"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();

    assert_eq!(result.get("pdf_path_count").unwrap().as_u64().unwrap(), 2);
    assert_eq!(result.get("image_count").unwrap().as_u64().unwrap(), 2);
    assert_eq!(result.get("pdf_path_success").unwrap(), true);

    let output = result.get("pdf_path").unwrap().as_str().unwrap();
    let pdf = lopdf::Document::load(output).unwrap();
    assert_eq!(pdf.get_pages().len(), 2);
}

#[tokio::test]
async fn image_to_pdf_accepts_source_key_with_base64_and_paths() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_to_pdf").unwrap();
    let dir = tempdir().unwrap();
    let out_dir = dir.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let front = dir.path().join("front.png");
    let back = dir.path().join("back.png");
    write_temp_png(&front, [100, 120, 140, 255], 80, 60);
    write_temp_png(&back, [30, 40, 50, 255], 64, 64);

    let front_data = std::fs::read(&front).unwrap();
    let back_data = std::fs::read(&back).unwrap();

    let front_b64 = base64::engine::general_purpose::STANDARD.encode(front_data);
    let back_b64 = base64::engine::general_purpose::STANDARD.encode(back_data);

    let ctx = ctx_with(vec![
        (
            "images",
            serde_json::json!([
                { "base64": front_b64 },
                { "path": back.to_string_lossy() },
                { "data": back_b64 }
            ]),
        ),
        ("out_path", serde_json::json!(out_dir.to_string_lossy().to_string())),
    ]);

    let output_path = format!("{}/from_context.pdf", ctx.get("out_path").unwrap().as_str().unwrap());
    let config = serde_json::json!({
        "source_key": "images",
        "output_path": output_path,
        "output_key": "pdf"
    });

    let result = node.execute(&config, ctx).await.unwrap();

    assert_eq!(result.get("pdf_count").unwrap().as_u64().unwrap(), 3);
    assert_eq!(result.get("image_count").unwrap().as_u64().unwrap(), 3);

    let created = result.get("pdf").unwrap().as_str().unwrap();
    let doc = lopdf::Document::load(created).unwrap();
    assert_eq!(doc.get_pages().len(), 3);
}

#[tokio::test]
async fn image_resize_generates_resized_file() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_resize").unwrap();
    let dir = tempdir().unwrap();
    let output_path = dir.path().join("resized.png");
    let source_path = dir.path().join("source.png");
    write_temp_png(&source_path, [10, 20, 30, 255], 120, 80);

    let config = serde_json::json!({
        "path": source_path.to_string_lossy(),
        "output_path": output_path.to_string_lossy(),
        "width": 96,
        "output_key": "resized"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();

    assert_eq!(result.get("resized_width").unwrap().as_u64().unwrap(), 96);
    assert!(result.get("resized_height").unwrap().as_u64().unwrap() > 0);
    assert_eq!(result.get("resized_format").unwrap(), "png");
    assert!(result.get("resized_success").unwrap().as_bool().unwrap());
    assert!(result.get("resized").unwrap().as_str().unwrap().ends_with(".png"));

    let image = image::ImageReader::open(output_path)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(image.width(), 96);
}

#[tokio::test]
async fn image_crop_generates_cropped_file() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_crop").unwrap();
    let dir = tempdir().unwrap();
    let output_path = dir.path().join("cropped.jpg");
    let source_path = dir.path().join("source.png");
    write_temp_png(&source_path, [20, 30, 40, 255], 160, 120);
    let source_path = source_path.to_string_lossy().to_string();
    let ctx = ctx_with(vec![("source", serde_json::json!(source_path))]);

    let config = serde_json::json!({
        "source_key": "source",
        "output_path": output_path.to_string_lossy(),
        "x": 10,
        "y": 8,
        "crop_width": 64,
        "crop_height": 64,
        "format": "jpeg",
        "output_key": "cropped"
    });

    let result = node.execute(&config, ctx).await.unwrap();

    assert_eq!(result.get("cropped_x").unwrap().as_u64().unwrap(), 10);
    assert_eq!(result.get("cropped_y").unwrap().as_u64().unwrap(), 8);
    assert_eq!(result.get("cropped_format").unwrap(), "jpeg");
    assert!(result.get("cropped_success").unwrap().as_bool().unwrap());

    let image = image::ImageReader::open(output_path)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(image.width(), 64);
    assert_eq!(image.height(), 64);
}

#[tokio::test]
async fn pdf_metadata_extracts_key_fields() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("pdf_metadata").unwrap();

    let sample_pdf = sample_root().join("Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf");

    let config = serde_json::json!({
        "path": sample_pdf.to_string_lossy(),
        "output_key": "meta"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let meta = result.get("meta").unwrap().as_object().unwrap();
    assert!(meta.contains_key("pages"));
    assert!(meta.get("pages").unwrap().as_u64().unwrap() > 0);
}

#[tokio::test]
async fn image_rotate_generates_rotated_file() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_rotate").unwrap();
    let dir = tempdir().unwrap();
    let output_path = dir.path().join("rotated.png");
    let source_path = dir.path().join("source.png");
    write_temp_png(&source_path, [200, 120, 80, 255], 100, 60);
    let source = image::ImageReader::open(&source_path)
        .unwrap()
        .decode()
        .unwrap();

    let config = serde_json::json!({
        "path": source_path.to_string_lossy(),
        "output_path": output_path.to_string_lossy(),
        "angle": 90,
        "output_key": "rotated"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();

    assert_eq!(result.get("rotated").unwrap().as_str().unwrap(), output_path.to_str().unwrap());
    assert_eq!(
        result.get("rotated_angle").unwrap().as_u64().unwrap(),
        90u64
    );
    assert_eq!(result.get("rotated_format").unwrap(), "png");

    let image = image::ImageReader::open(output_path)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(image.width(), source.height());
    assert_eq!(image.height(), source.width());
}

#[tokio::test]
async fn image_flip_and_grayscale() {
    let reg = NodeRegistry::with_builtins();

    let flip = reg.get("image_flip").unwrap();
    let gray = reg.get("image_grayscale").unwrap();
    let dir = tempdir().unwrap();
    let source_path = dir.path().join("source.png");
    write_temp_png(&source_path, [120, 90, 30, 255], 90, 70);
    let flip_path = dir.path().join("flipped.png");
    let gray_path = dir.path().join("grayscale.png");
    let source = image::ImageReader::open(&source_path)
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba8();

    let flip_config = serde_json::json!({
        "path": source_path.to_string_lossy(),
        "output_path": flip_path.to_string_lossy(),
        "direction": "horizontal",
        "output_key": "flipped"
    });

    let flip_result = flip.execute(&flip_config, empty_ctx()).await.unwrap();
    assert!(flip_result.get("flipped_success").unwrap().as_bool().unwrap());
    assert_eq!(
        flip_result.get("flipped_width").unwrap().as_u64().unwrap(),
        u64::from(source.width())
    );
    assert_eq!(
        flip_result.get("flipped_height").unwrap().as_u64().unwrap(),
        u64::from(source.height())
    );

    let flipped = image::ImageReader::open(flip_path)
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba8();
    let expected = source.get_pixel(source.width() - 1, 0);
    assert_eq!(*flipped.get_pixel(0, 0), *expected);

    let gray_config = serde_json::json!({
        "path": source_path.to_string_lossy(),
        "output_path": gray_path.to_string_lossy(),
        "output_key": "gray"
    });

    let gray_result = gray.execute(&gray_config, empty_ctx()).await.unwrap();
    assert_eq!(gray_result.get("gray_format").unwrap(), "png");
    assert!(gray_result.get("gray_success").unwrap().as_bool().unwrap());

    let gray_image = image::ImageReader::open(gray_path)
        .unwrap()
        .decode()
        .unwrap()
        .to_rgb8();
    let first = gray_image.get_pixel(3, 7);
    assert_eq!(first[0], first[1]);
    assert_eq!(first[1], first[2]);
}
