use std::io::Write;

use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TRANSITIONAL_IMAGE_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const STRICT_IMAGE_RELATIONSHIP: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/image";

struct ArtifactEnvironment(Option<std::ffi::OsString>);

impl ArtifactEnvironment {
    fn set(path: &std::path::Path) -> Self {
        let original = std::env::var_os("IRONFLOW_ARTIFACT_DIR");
        // SAFETY: artifact-environment mutation in this test binary is serialized.
        unsafe { std::env::set_var("IRONFLOW_ARTIFACT_DIR", path) };
        Self(original)
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

fn write_pptx(path: &std::path::Path, relationships: &str, parts: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    archive
        .start_file("ppt/slides/slide1.xml", options)
        .unwrap();
    archive
        .write_all(b"<sld><pic><cNvPr descr=\"image\"/><blip embed=\"rId1\"/></pic></sld>")
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

async fn extract(path: &std::path::Path) -> anyhow::Result<NodeOutput> {
    NodeRegistry::with_builtins()
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
}

fn assert_no_artifacts(artifact_dir: &std::path::Path) {
    assert_eq!(
        std::fs::read_dir(artifact_dir.join("sha256"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn pptx_decodes_strict_image_target_and_prefers_package_mime_override() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let path = directory.path().join("escaped-target.pptx");
    let media = b"package-typed-image";
    let relationships = format!(
        "<Relationships><Relationship Id=\"rId1\" Type=\"{STRICT_IMAGE_RELATIONSHIP}\" \
         Target=\"../media/image&#x31;.blob\"/></Relationships>"
    );
    let content_types = br#"<Types>
        <Default Extension="BLOB" ContentType="image/jpeg"/>
        <Override PartName="/PPT/MEDIA/IMAGE1.BLOB" ContentType="image/png"/>
    </Types>"#;
    write_pptx(
        &path,
        &relationships,
        &[
            ("[Content_Types].xml", content_types),
            ("ppt/media/image1.blob", media),
        ],
    );

    let output = extract(&path).await.unwrap();
    let image = &output["content"]["slides"][0]["elements"][0];
    assert_eq!(image["embedded_path"], "ppt/media/image1.blob");
    assert_eq!(image["artifact"]["mime_type"], "image/png");
    let digest = image["artifact"]["sha256"].as_str().unwrap();
    assert_eq!(
        std::fs::read(artifact_dir.join("sha256").join(digest)).unwrap(),
        media
    );
}

#[tokio::test]
async fn pptx_rejects_duplicate_relationship_ids_before_publication() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let path = directory.path().join("duplicate-relationships.pptx");
    let relationships = format!(
        "<Relationships>\
         <Relationship Id=\"rId1\" Type=\"{TRANSITIONAL_IMAGE_RELATIONSHIP}\" Target=\"../media/first.png\"/>\
         <Relationship Id=\"rId1\" Type=\"{TRANSITIONAL_IMAGE_RELATIONSHIP}\" Target=\"../media/second.png\"/>\
         </Relationships>"
    );
    write_pptx(
        &path,
        &relationships,
        &[
            ("ppt/media/first.png", b"first"),
            ("ppt/media/second.png", b"second"),
        ],
    );

    let error = extract(&path).await.unwrap_err().to_string();
    assert!(
        error.contains("duplicate slide relationship Id 'rId1'"),
        "{error}"
    );
    assert_no_artifacts(&artifact_dir);
}

#[tokio::test]
async fn pptx_rejects_invalid_package_mime_before_media_publication() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let path = directory.path().join("invalid-content-type.pptx");
    let relationships = format!(
        "<Relationships><Relationship Id=\"rId1\" Type=\"{TRANSITIONAL_IMAGE_RELATIONSHIP}\" \
         Target=\"../media/image.png\"/></Relationships>"
    );
    write_pptx(
        &path,
        &relationships,
        &[
            (
                "[Content_Types].xml",
                br#"<Types><Default Extension="png" ContentType=" image/png"/></Types>"#,
            ),
            ("ppt/media/image.png", b"must-not-be-published"),
        ],
    );

    let error = extract(&path).await.unwrap_err().to_string();
    assert!(error.contains("invalid package ContentType"), "{error}");
    assert_no_artifacts(&artifact_dir);
}

#[tokio::test]
async fn pptx_does_not_expose_non_image_or_external_targets() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);

    let non_image_path = directory.path().join("non-image.pptx");
    let non_image_relationship = "<Relationships><Relationship Id=\"rId1\" \
         Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" \
         Target=\"../media/not-an-image.bin\"/></Relationships>";
    write_pptx(
        &non_image_path,
        non_image_relationship,
        &[("ppt/media/not-an-image.bin", b"must-not-be-published")],
    );
    let output = extract(&non_image_path).await.unwrap();
    let image = &output["content"]["slides"][0]["elements"][0];
    assert_eq!(image["embed_id"], "rId1");
    assert!(image.get("embedded_path").is_none());
    assert!(image.get("artifact").is_none());
    assert_no_artifacts(&artifact_dir);

    let external_path = directory.path().join("external-image.pptx");
    let external_relationship = format!(
        "<Relationships><Relationship Id=\"rId1\" Type=\"{TRANSITIONAL_IMAGE_RELATIONSHIP}\" \
         Target=\"https://example.invalid/image.png\" TargetMode=\"External\"/></Relationships>"
    );
    write_pptx(&external_path, &external_relationship, &[]);
    let external_output = extract(&external_path).await.unwrap();
    let external_image = &external_output["content"]["slides"][0]["elements"][0];
    assert!(external_image.get("embedded_path").is_none());
    assert!(external_image.get("artifact").is_none());
    assert_no_artifacts(&artifact_dir);
}

#[tokio::test]
async fn pptx_rejects_malformed_internal_relationships_before_publication() {
    let _lock = ENV_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let _environment = ArtifactEnvironment::set(&artifact_dir);
    let image_type = TRANSITIONAL_IMAGE_RELATIONSHIP;
    let cases = [
        (
            "unknown-mode",
            format!(
                "<Relationships><Relationship Id=\"rId1\" Type=\"{image_type}\" \
                 Target=\"../media/image.png\" TargetMode=\"Sideways\"/></Relationships>"
            ),
            "invalid relationship TargetMode",
        ),
        (
            "empty-target",
            format!(
                "<Relationships><Relationship Id=\"rId1\" Type=\"{image_type}\" \
                 Target=\"\"/></Relationships>"
            ),
            "missing required Target attribute",
        ),
        (
            "terminal-dot",
            format!(
                "<Relationships><Relationship Id=\"rId1\" Type=\"{image_type}\" \
                 Target=\"../media/..\"/></Relationships>"
            ),
            "target names a directory",
        ),
        (
            "scheme-target",
            format!(
                "<Relationships><Relationship Id=\"rId1\" Type=\"{image_type}\" \
                 Target=\"https://example.invalid/image.png\"/></Relationships>"
            ),
            "target has a URI scheme",
        ),
        (
            "query-target",
            format!(
                "<Relationships><Relationship Id=\"rId1\" Type=\"{image_type}\" \
                 Target=\"../media/image.png?download=1\"/></Relationships>"
            ),
            "target has a query or fragment",
        ),
        (
            "empty-segment",
            format!(
                "<Relationships><Relationship Id=\"rId1\" Type=\"{image_type}\" \
                 Target=\"../media//image.png\"/></Relationships>"
            ),
            "target has an empty segment",
        ),
        (
            "missing-id",
            format!(
                "<Relationships><Relationship Type=\"{image_type}\" \
                 Target=\"../media/image.png\"/></Relationships>"
            ),
            "missing required Id attribute",
        ),
        (
            "missing-type",
            "<Relationships><Relationship Id=\"rId1\" \
             Target=\"../media/image.png\"/></Relationships>"
                .to_string(),
            "missing required Type attribute",
        ),
        (
            "missing-target",
            format!(
                "<Relationships><Relationship Id=\"rId1\" \
                 Type=\"{image_type}\"/></Relationships>"
            ),
            "missing required Target attribute",
        ),
        (
            "malformed-entity",
            format!(
                "<Relationships><Relationship Id=\"rId1\" Type=\"{image_type}\" \
                 Target=\"../media/image&bogus;.png\"/></Relationships>"
            ),
            "invalid relationship attribute value",
        ),
    ];

    for (name, relationships, expected) in cases {
        let path = directory.path().join(format!("{name}.pptx"));
        write_pptx(&path, &relationships, &[]);
        let error = extract(&path).await.unwrap_err().to_string();
        assert!(error.contains(expected), "{name}: {error}");
        assert_no_artifacts(&artifact_dir);
    }
}
