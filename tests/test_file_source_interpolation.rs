//! File-source parameters are interpolated once, at the configuration
//! boundary; values read from context are data (IF-186).
//!
//! Every case keeps only the file whose name is the literal placeholder text,
//! so a second expansion pass makes the node open a file that does not exist.

use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use lopdf::{Document, Object, Stream, dictionary};

const LITERAL: &str = "${ctx.api_token}";

fn context(pairs: &[(&str, serde_json::Value)]) -> Context {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn literal_path(directory: &Path, extension: &str) -> String {
    directory
        .join(format!("{LITERAL}.{extension}"))
        .to_str()
        .unwrap()
        .to_owned()
}

async fn run(
    node: &str,
    config: serde_json::Value,
    ctx: &Context,
) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
    NodeRegistry::with_builtins()
        .get(node)
        .unwrap()
        .execute(&config, ctx)
        .await
}

/// The three IF-186 cases for a node that takes `path` or `source_key`.
async fn assert_single_pass(node: &str, extra: serde_json::Value, file: &str) {
    let with = |mut config: serde_json::Value| {
        for (key, value) in extra.as_object().unwrap() {
            config[key] = value.clone();
        }
        config
    };
    let upload = context(&[
        ("upload", serde_json::json!(file)),
        ("api_token", serde_json::json!("s3cr3t-token")),
    ]);

    run(
        node,
        with(serde_json::json!({ "path": "${ctx.upload}" })),
        &upload,
    )
    .await
    .unwrap_or_else(|error| panic!("{node}: placeholder in a context value expanded: {error}"));

    run(
        node,
        with(serde_json::json!({ "source_key": "upload" })),
        &upload,
    )
    .await
    .unwrap_or_else(|error| panic!("{node}: source_key value used as a template: {error}"));

    let escaped = file.replace(LITERAL, &format!("\\{LITERAL}"));
    run(node, with(serde_json::json!({ "path": escaped })), &upload)
        .await
        .unwrap_or_else(|error| panic!("{node}: escaped placeholder expanded: {error}"));

    // A failure names the configured value, never a second expansion of it.
    for config in [
        serde_json::json!({ "path": "${ctx.upload}.missing" }),
        serde_json::json!({ "source_key": "missing" }),
    ] {
        let missing = context(&[
            ("upload", serde_json::json!(file)),
            ("missing", serde_json::json!(format!("{file}.missing"))),
            ("api_token", serde_json::json!("s3cr3t-token")),
        ]);
        let error = run(node, with(config), &missing)
            .await
            .unwrap_err()
            .to_string();
        assert!(!error.contains("s3cr3t-token"), "{node}: {error}");
    }
}

fn create_pdf(path: &str) {
    let mut document = Document::new();
    let pages_id = document.new_object_id();
    let content = document.add_object(Stream::new(dictionary! {}, b"BT (x) Tj ET".to_vec()));
    let page = document.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()]
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    document.save(path).unwrap();
}

#[tokio::test]
async fn extract_html_interpolates_its_source_once() {
    let directory = tempfile::tempdir().unwrap();
    let file = literal_path(directory.path(), "html");
    std::fs::write(&file, "<p>literal</p>").unwrap();
    assert_single_pass("extract_html", serde_json::json!({}), &file).await;
}

#[tokio::test]
async fn pdf_metadata_interpolates_its_source_once() {
    let directory = tempfile::tempdir().unwrap();
    let file = literal_path(directory.path(), "pdf");
    create_pdf(&file);
    assert_single_pass("pdf_metadata", serde_json::json!({}), &file).await;
}

#[tokio::test]
async fn image_resize_interpolates_its_source_once() {
    let directory = tempfile::tempdir().unwrap();
    let file = literal_path(directory.path(), "png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([1, 2, 3]))
        .save(&file)
        .unwrap();
    let output = directory.path().join("out.png");
    assert_single_pass(
        "image_resize",
        serde_json::json!({ "output_path": output, "width": 2 }),
        &file,
    )
    .await;
}

#[tokio::test]
async fn image_source_objects_read_from_context_are_data() {
    let directory = tempfile::tempdir().unwrap();
    let file = literal_path(directory.path(), "png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([1, 2, 3]))
        .save(&file)
        .unwrap();
    let ctx = context(&[
        ("image", serde_json::json!({ "path": file })),
        ("images", serde_json::json!([file, { "path": file }])),
        ("api_token", serde_json::json!("s3cr3t-token")),
    ]);
    run(
        "image_resize",
        serde_json::json!({
            "source_key": "image",
            "output_path": directory.path().join("out.png"),
            "width": 2
        }),
        &ctx,
    )
    .await
    .unwrap();
    let output = run(
        "image_to_pdf",
        serde_json::json!({
            "source_key": "images",
            "output_path": directory.path().join("out.pdf")
        }),
        &ctx,
    )
    .await
    .unwrap();
    assert_eq!(output["image_count"], 2);
}

#[tokio::test]
async fn configured_source_lists_still_interpolate_once() {
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("page.png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([1, 2, 3]))
        .save(&image)
        .unwrap();
    let pdf = directory.path().join("page.pdf");
    create_pdf(pdf.to_str().unwrap());
    let ctx = context(&[("dir", serde_json::json!(directory.path()))]);
    let output = run(
        "image_to_pdf",
        serde_json::json!({
            "sources": ["${ctx.dir}/page.png", { "path": "${ctx.dir}/page.png" }],
            "output_path": directory.path().join("images.pdf")
        }),
        &ctx,
    )
    .await
    .unwrap();
    assert_eq!(output["image_count"], 2);
    run(
        "pdf_merge",
        serde_json::json!({
            "files": ["${ctx.dir}/page.pdf", "${ctx.dir}/page.pdf"],
            "output_path": directory.path().join("merged.pdf")
        }),
        &ctx,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn pdf_merge_sources_read_from_context_are_data() {
    let directory = tempfile::tempdir().unwrap();
    let file = literal_path(directory.path(), "pdf");
    create_pdf(&file);
    let ctx = context(&[
        ("files", serde_json::json!([file, file])),
        ("api_token", serde_json::json!("s3cr3t-token")),
    ]);
    run(
        "pdf_merge",
        serde_json::json!({
            "source_key": "files",
            "output_path": directory.path().join("merged.pdf")
        }),
        &ctx,
    )
    .await
    .unwrap();
}
