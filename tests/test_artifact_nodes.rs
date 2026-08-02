use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct ArtifactEnvironment(Option<std::ffi::OsString>);

impl ArtifactEnvironment {
    fn set(path: &Path) -> Self {
        let original = std::env::var_os("IRONFLOW_ARTIFACT_DIR");
        // SAFETY: artifact-environment mutation in this test binary is serialized.
        unsafe { std::env::set_var("IRONFLOW_ARTIFACT_DIR", path) };
        Self(original)
    }
}

impl Drop for ArtifactEnvironment {
    fn drop(&mut self) {
        // SAFETY: the environment lock remains held until this guard drops.
        unsafe {
            match &self.0 {
                Some(value) => std::env::set_var("IRONFLOW_ARTIFACT_DIR", value),
                None => std::env::remove_var("IRONFLOW_ARTIFACT_DIR"),
            }
        }
    }
}

#[tokio::test]
async fn read_file_artifact_can_feed_an_extractor_without_base64() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let source = directory.path().join("source.html");
    std::fs::write(&source, b"<main><h1>Artifact input</h1></main>").unwrap();
    let registry = NodeRegistry::with_builtins();

    let read = registry
        .get("read_file")
        .unwrap()
        .execute(
            &serde_json::json!({
                "path": source,
                "encoding": "artifact",
                "mime_type": "text/html",
                "output_key": "source"
            }),
            &Context::new(),
        )
        .await
        .unwrap();
    assert!(!read.contains_key("source_content"));
    let descriptor = read["source_artifact"].clone();
    let artifact: ironflow::artifacts::ArtifactRef =
        serde_json::from_value(descriptor.clone()).unwrap();
    assert_eq!(artifact.mime_type.as_deref(), Some("text/html"));
    assert_eq!(
        std::fs::read(
            ironflow::artifacts::LocalArtifactStore::new(&artifact_dir)
                .unwrap()
                .resolve(&artifact)
                .unwrap()
        )
        .unwrap(),
        b"<main><h1>Artifact input</h1></main>"
    );

    let context: Context = [("source".to_owned(), descriptor)].into_iter().collect();
    let extracted = registry
        .get("extract_html")
        .unwrap()
        .execute(&serde_json::json!({ "source_key": "source" }), &context)
        .await
        .unwrap();
    assert!(
        extracted["content"]
            .as_str()
            .unwrap()
            .contains("Artifact input")
    );
}

#[tokio::test]
async fn extractor_rejects_same_size_artifact_replacement() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let source = directory.path().join("source.html");
    std::fs::write(&source, b"<p>trusted</p>").unwrap();
    let registry = NodeRegistry::with_builtins();
    let read = registry
        .get("read_file")
        .unwrap()
        .execute(
            &serde_json::json!({"path": source, "encoding": "artifact"}),
            &Context::new(),
        )
        .await
        .unwrap();
    let descriptor = read["file_artifact"].clone();
    let artifact: ironflow::artifacts::ArtifactRef =
        serde_json::from_value(descriptor.clone()).unwrap();
    let stored = ironflow::artifacts::LocalArtifactStore::new(&artifact_dir)
        .unwrap()
        .resolve(&artifact)
        .unwrap();
    make_writable(&stored);
    std::fs::write(&stored, b"<p>hostile</p>").unwrap();

    let context = Context::from([("source".to_owned(), descriptor)]);
    let error = registry
        .get("extract_html")
        .unwrap()
        .execute(&serde_json::json!({"source_key": "source"}), &context)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("digest verification"), "{error}");
}

#[tokio::test]
async fn image_to_pdf_accepts_an_artifact_descriptor() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let source = directory.path().join("source.png");
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        8,
        8,
        image::Rgba([20, 40, 60, 255]),
    ))
    .save(&source)
    .unwrap();
    let registry = NodeRegistry::with_builtins();
    let read = registry
        .get("read_file")
        .unwrap()
        .execute(
            &serde_json::json!({
                "path": source,
                "encoding": "artifact",
                "mime_type": "image/png"
            }),
            &Context::new(),
        )
        .await
        .unwrap();
    let output_path = directory.path().join("artifact-image.pdf");
    let output = registry
        .get("image_to_pdf")
        .unwrap()
        .execute(
            &serde_json::json!({
                "sources": [read["file_artifact"].clone()],
                "output_path": output_path
            }),
            &Context::new(),
        )
        .await
        .unwrap();
    assert_eq!(output["image_count"], 1);
    assert_eq!(
        lopdf::Document::load(output_path)
            .unwrap()
            .get_pages()
            .len(),
        1
    );
}

#[tokio::test]
async fn image_metadata_sniffs_an_extensionless_artifact() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let source = directory.path().join("source.png");
    let image = image::RgbaImage::from_pixel(2, 3, image::Rgba([1, 2, 3, 255]));
    image.save(&source).unwrap();
    let registry = NodeRegistry::with_builtins();

    let read = registry
        .get("read_file")
        .unwrap()
        .execute(
            &serde_json::json!({
                "path": source,
                "encoding": "artifact",
                "mime_type": "image/png",
                "output_key": "image"
            }),
            &Context::new(),
        )
        .await
        .unwrap();
    let context: Context = [("image".to_owned(), read["image_artifact"].clone())]
        .into_iter()
        .collect();
    let metadata = registry
        .get("image_metadata")
        .unwrap()
        .execute(
            &serde_json::json!({ "source_key": "image", "output_key": "meta" }),
            &context,
        )
        .await
        .unwrap();

    assert_eq!(metadata["meta_width"], 2);
    assert_eq!(metadata["meta_height"], 3);
    assert_eq!(metadata["meta_format"], "png");
}

#[cfg(unix)]
fn make_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn make_writable(path: &Path) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(false);
    std::fs::set_permissions(path, permissions).unwrap();
}
