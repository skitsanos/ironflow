use std::io::Write;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TRANSITIONAL_IMAGE_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

struct ArtifactEnvironment(Option<std::ffi::OsString>);

impl ArtifactEnvironment {
    fn set(path: &std::path::Path) -> Self {
        let original = std::env::var_os("IRONFLOW_ARTIFACT_DIR");
        // SAFETY: artifact-environment mutation in this test binary is serialized by ENV_LOCK.
        unsafe { std::env::set_var("IRONFLOW_ARTIFACT_DIR", path) };
        Self(original)
    }
}

struct OutputLimitEnvironment(Option<std::ffi::OsString>);

impl OutputLimitEnvironment {
    fn set(value: &str) -> Self {
        let original = std::env::var_os("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES");
        // SAFETY: extraction environment mutation in this test binary is serialized.
        unsafe { std::env::set_var("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", value) };
        Self(original)
    }
}

impl Drop for OutputLimitEnvironment {
    fn drop(&mut self) {
        // SAFETY: the guard is dropped while ENV_LOCK remains held.
        unsafe {
            match &self.0 {
                Some(value) => std::env::set_var("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", value),
                None => std::env::remove_var("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"),
            }
        }
    }
}

impl Drop for ArtifactEnvironment {
    fn drop(&mut self) {
        // SAFETY: the guard is dropped while ENV_LOCK remains held.
        unsafe {
            match &self.0 {
                Some(value) => std::env::set_var("IRONFLOW_ARTIFACT_DIR", value),
                None => std::env::remove_var("IRONFLOW_ARTIFACT_DIR"),
            }
        }
    }
}

fn write_pptx(path: &std::path::Path, media: Option<&[u8]>) {
    let relationships = format!(
        "<Relationships><Relationship Id=\"rId1\" Type=\"{TRANSITIONAL_IMAGE_RELATIONSHIP}\" \
         Target=\"../media/image.png\"/></Relationships>"
    );
    let parts = media
        .map(|media| vec![("ppt/media/image.png", media)])
        .unwrap_or_default();
    write_pptx_parts(path, &relationships, &parts);
}

fn write_pptx_parts(path: &std::path::Path, relationships: &str, parts: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    archive
        .start_file("ppt/slides/slide1.xml", options)
        .unwrap();
    archive
        .write_all(
            b"<sld><pic><cNvPr descr=\"first\"/><blip embed=\"rId1\"/></pic>\
              <pic><cNvPr descr=\"second\"/><blip embed=\"rId1\"/></pic></sld>",
        )
        .unwrap();
    archive
        .start_file("ppt/slides/_rels/slide1.xml.rels", options)
        .unwrap();
    archive.write_all(relationships.as_bytes()).unwrap();
    for (name, contents) in parts {
        archive.start_file(*name, options).unwrap();
        archive.write_all(contents).unwrap();
    }
    archive.finish().unwrap();
}

#[tokio::test]
async fn pptx_media_is_streamed_to_one_content_addressed_artifact() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let path = directory.path().join("media.pptx");
    let media = b"bounded-png-payload";
    write_pptx(&path, Some(media));

    let node = NodeRegistry::with_builtins().get("extract_pptx").unwrap();
    let output = node
        .execute(
            &serde_json::json!({
                "path": path,
                "format": "json",
                "media_mode": "artifact"
            }),
            &Context::new(),
        )
        .await
        .unwrap();
    let images = output["content"]["slides"][0]["elements"]
        .as_array()
        .unwrap();
    assert_eq!(images.len(), 2);
    let first = &images[0]["artifact"];
    let second = &images[1]["artifact"];
    assert_eq!(
        first, second,
        "repeated media references must be deduplicated"
    );
    assert_eq!(first["size_bytes"], media.len() as u64);
    assert_eq!(first["mime_type"], "image/png");
    assert!(images[0].get("media_b64").is_none());
    assert!(images[0].get("mime_type").is_none());

    let digest = first["sha256"].as_str().unwrap();
    assert_eq!(
        std::fs::read(artifact_dir.join("sha256").join(digest)).unwrap(),
        media
    );
    assert_eq!(
        std::fs::read_dir(artifact_dir.join("sha256"))
            .unwrap()
            .count(),
        1
    );
}

#[tokio::test]
async fn pptx_media_is_only_read_in_artifact_mode() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let _environment = ArtifactEnvironment::set(&directory.path().join("artifacts"));
    let path = directory.path().join("missing-media.pptx");
    write_pptx(&path, None);

    let node = NodeRegistry::with_builtins().get("extract_pptx").unwrap();
    let output = node
        .execute(
            &serde_json::json!({ "path": path, "format": "json" }),
            &Context::new(),
        )
        .await
        .unwrap();
    let image = &output["content"]["slides"][0]["elements"][0];
    assert_eq!(image["embedded_path"], "ppt/media/image.png");
    assert!(image.get("artifact").is_none());

    let error = node
        .execute(
            &serde_json::json!({
                "path": path,
                "format": "json",
                "media_mode": "artifact"
            }),
            &Context::new(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("referenced image archive part is missing"),
        "{error}"
    );
}

#[tokio::test]
async fn pptx_rejects_descriptor_budget_before_publishing_media() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let _output_limit = OutputLimitEnvironment::set("256");
    let path = directory.path().join("budget.pptx");
    write_pptx(&path, Some(b"must-not-be-published"));

    let error = NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(
            &serde_json::json!({
                "path": path,
                "format": "json",
                "media_mode": "artifact"
            }),
            &Context::new(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"),
        "{error}"
    );
    assert!(error.contains("PPTX artifact descriptors"), "{error}");
    assert_eq!(
        std::fs::read_dir(artifact_dir.join("sha256"))
            .unwrap()
            .count(),
        0
    );
}
