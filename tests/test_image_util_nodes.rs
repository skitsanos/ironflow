use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use std::collections::HashMap;
use tempfile::tempdir;

fn empty_ctx() -> Context {
    HashMap::new()
}

fn create_test_image(path: &std::path::Path, width: u32, height: u32) {
    let img = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
    });
    img.save(path).unwrap();
}

#[tokio::test]
async fn image_metadata_basic() {
    let dir = tempdir().unwrap();
    let img_path = dir.path().join("test.png");
    create_test_image(&img_path, 200, 100);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_metadata").unwrap();

    let config = serde_json::json!({
        "path": img_path.to_string_lossy()
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        result
            .get("image_metadata_width")
            .unwrap()
            .as_u64()
            .unwrap(),
        200
    );
    assert_eq!(
        result
            .get("image_metadata_height")
            .unwrap()
            .as_u64()
            .unwrap(),
        100
    );
    assert_eq!(
        result
            .get("image_metadata_format")
            .unwrap()
            .as_str()
            .unwrap(),
        "png"
    );
    assert!(result.contains_key("image_metadata_color_type"));
}

#[tokio::test]
async fn image_metadata_custom_output_key() {
    let dir = tempdir().unwrap();
    let img_path = dir.path().join("test.png");
    create_test_image(&img_path, 300, 150);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_metadata").unwrap();

    let config = serde_json::json!({
        "path": img_path.to_string_lossy(),
        "output_key": "my_img"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(result.get("my_img_width").unwrap().as_u64().unwrap(), 300);
    assert_eq!(result.get("my_img_height").unwrap().as_u64().unwrap(), 150);
    assert_eq!(
        result.get("my_img_format").unwrap().as_str().unwrap(),
        "png"
    );
    assert!(result.contains_key("my_img_color_type"));
}

#[tokio::test]
async fn image_metadata_missing_file_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_metadata").unwrap();

    let config = serde_json::json!({
        "path": "/tmp/nonexistent_image_12345.png"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn image_convert_png_to_jpeg() {
    let dir = tempdir().unwrap();
    let img_path = dir.path().join("input.png");
    let out_path = dir.path().join("output.jpg");
    create_test_image(&img_path, 100, 80);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_convert").unwrap();

    let config = serde_json::json!({
        "path": img_path.to_string_lossy(),
        "output_path": out_path.to_string_lossy(),
        "quality": 90
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(out_path.exists());
    assert!(
        result
            .get("image_convert_success")
            .unwrap()
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        result
            .get("image_convert_format")
            .unwrap()
            .as_str()
            .unwrap(),
        "jpg"
    );
}

#[tokio::test]
async fn image_convert_missing_file_error() {
    let dir = tempdir().unwrap();
    let out_path = dir.path().join("output.jpg");

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_convert").unwrap();

    let config = serde_json::json!({
        "path": "/tmp/nonexistent_image_12345.png",
        "output_path": out_path.to_string_lossy()
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn image_watermark_basic() {
    let dir = tempdir().unwrap();
    let img_path = dir.path().join("input.png");
    let out_path = dir.path().join("watermarked.png");
    create_test_image(&img_path, 200, 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_watermark").unwrap();

    let config = serde_json::json!({
        "path": img_path.to_string_lossy(),
        "output_path": out_path.to_string_lossy(),
        "text": "Sample Watermark",
        "opacity": 0.5
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(out_path.exists());
    assert!(
        result
            .get("image_watermark_success")
            .unwrap()
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        result
            .get("image_watermark_text")
            .unwrap()
            .as_str()
            .unwrap(),
        "Sample Watermark"
    );
}

#[tokio::test]
async fn image_watermark_positions() {
    let dir = tempdir().unwrap();
    let img_path = dir.path().join("input.png");
    create_test_image(&img_path, 200, 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_watermark").unwrap();

    for position in &[
        "top-left",
        "top-right",
        "bottom-left",
        "bottom-right",
        "center",
    ] {
        let out_path = dir.path().join(format!("watermarked_{}.png", position));
        let config = serde_json::json!({
            "path": img_path.to_string_lossy(),
            "output_path": out_path.to_string_lossy(),
            "text": "Test",
            "position": position
        });

        let result = node.execute(&config, empty_ctx()).await.unwrap();
        assert!(
            out_path.exists(),
            "Output missing for position {}",
            position
        );
        assert!(
            result
                .get("image_watermark_success")
                .unwrap()
                .as_bool()
                .unwrap()
        );
    }
}
