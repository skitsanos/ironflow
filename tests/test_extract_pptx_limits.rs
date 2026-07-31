use std::io::Write;
use std::path::{Path, PathBuf};

use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;

const ITEM_LIMIT: &str = "IRONFLOW_MAX_EXTRACT_ITEMS";
const OUTPUT_LIMIT: &str = "IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES";
const ZIP_BYTE_LIMIT: &str = "IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES";
const ZIP_ENTRY_LIMIT: &str = "IRONFLOW_MAX_ZIP_ENTRIES";
const FILE_LIMIT: &str = "IRONFLOW_MAX_FILE_BYTES";

struct Environment {
    original: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl Environment {
    fn capture(names: &[&'static str]) -> Self {
        Self {
            original: names
                .iter()
                .map(|name| (*name, std::env::var_os(name)))
                .collect(),
        }
    }

    fn set(name: &'static str, value: &str) {
        unsafe { std::env::set_var(name, value) };
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        for (name, value) in &self.original {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

fn write_pptx(path: &Path, parts: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in parts {
        archive.start_file(*name, options).unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap();
}

fn compact_slide(text: &str) -> String {
    format!("<sld><sp><txBody><p><r><t>{text}</t></r></p></txBody></sp></sld>")
}

async fn execute(path: &Path, extra: serde_json::Value) -> anyhow::Result<NodeOutput> {
    let node = NodeRegistry::with_builtins().get("extract_pptx").unwrap();
    let mut config = serde_json::json!({ "path": path });
    config.as_object_mut().unwrap().extend(
        extra
            .as_object()
            .expect("PPTX test configuration must be an object")
            .clone(),
    );
    node.execute(&config, &Context::new()).await
}

fn corrupt_stored_payload(path: &Path, payload: &[u8]) {
    let mut bytes = std::fs::read(path).unwrap();
    let offset = bytes
        .windows(payload.len())
        .position(|window| window == payload)
        .expect("stored ZIP payload must be present verbatim");
    bytes[offset] ^= 0x01;
    std::fs::write(path, bytes).unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn extract_pptx_enforces_resource_archive_and_config_boundaries() {
    let _environment = Environment::capture(&[
        ITEM_LIMIT,
        OUTPUT_LIMIT,
        ZIP_BYTE_LIMIT,
        ZIP_ENTRY_LIMIT,
        FILE_LIMIT,
    ]);
    Environment::set(ITEM_LIMIT, "10000");
    Environment::set(OUTPUT_LIMIT, "1048576");
    Environment::set(ZIP_BYTE_LIMIT, "1048576");
    Environment::set(ZIP_ENTRY_LIMIT, "100");
    Environment::set(FILE_LIMIT, "1048576");

    config_errors_precede_io().await;

    let directory = tempfile::tempdir().unwrap();
    let normal = directory.path().join("normal.pptx");
    let slide = compact_slide("bounded");
    write_pptx(&normal, &[("ppt/slides/slide1.xml", slide.as_bytes())]);
    let output = execute(&normal, serde_json::json!({ "format": "json" }))
        .await
        .unwrap();
    assert_eq!(output["content"]["slides"].as_array().unwrap().len(), 1);

    Environment::set(ITEM_LIMIT, "1");
    let error = execute(&normal, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(ITEM_LIMIT), "{error}");
    Environment::set(ITEM_LIMIT, "10000");

    let compact = directory.path().join("compact.pptx");
    let tiny_slide = compact_slide("x");
    write_pptx(
        &compact,
        &[("ppt/slides/slide1.xml", tiny_slide.as_bytes())],
    );
    Environment::set(OUTPUT_LIMIT, "100");
    let error = execute(&compact, serde_json::json!({ "format": "json" }))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(OUTPUT_LIMIT), "{error}");
    Environment::set(OUTPUT_LIMIT, "1048576");

    reject_invalid_packages(directory.path(), &slide).await;
    reject_corrupt_optional_part(directory.path(), &slide).await;

    Environment::set(ZIP_BYTE_LIMIT, "100");
    let cumulative = directory.path().join("cumulative.pptx");
    let notes = b"<notes><t>more retained content here</t></notes>";
    write_pptx(
        &cumulative,
        &[
            ("ppt/slides/slide1.xml", slide.as_bytes()),
            ("ppt/notesSlides/notesSlide1.xml", notes),
        ],
    );
    let error = execute(&cumulative, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(ZIP_BYTE_LIMIT), "{error}");
    Environment::set(ZIP_BYTE_LIMIT, "1048576");

    #[cfg(unix)]
    reject_special_inputs(directory.path(), &normal).await;
}

async fn config_errors_precede_io() {
    let absent = PathBuf::from("/does/not/exist.pptx");
    let error = execute(&absent, serde_json::json!({ "include_image_bytes": 7 }))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("'include_image_bytes' must be a boolean"),
        "{error}"
    );

    let error = execute(
        &absent,
        serde_json::json!({
            "output_key": "same",
            "metadata_key": "same"
        }),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("must name different context keys"),
        "{error}"
    );

    let error = execute(
        &absent,
        serde_json::json!({
            "include_image_bytes": true
        }),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("no longer supported"), "{error}");
    assert!(error.contains("media_mode"), "{error}");

    let error = execute(&absent, serde_json::json!({ "media_mode": 7 }))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("'media_mode' must be a string"), "{error}");

    let error = execute(&absent, serde_json::json!({ "media_mode": "inline" }))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("Must be 'none' or 'artifact'"), "{error}");

    let error = execute(
        &absent,
        serde_json::json!({ "format": "text", "media_mode": "artifact" }),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("requires format = 'json'"), "{error}");
}

async fn reject_invalid_packages(directory: &Path, slide: &str) {
    let empty = directory.join("empty.pptx");
    write_pptx(&empty, &[("[Content_Types].xml", b"<Types/>")]);
    let error = execute(&empty, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("contains no slide parts"), "{error}");

    let invalid_notes = directory.join("invalid-notes.pptx");
    write_pptx(
        &invalid_notes,
        &[
            ("ppt/slides/slide1.xml", slide.as_bytes()),
            ("ppt/notesSlides/notesSlide1.xml", b""),
        ],
    );
    let error = execute(&invalid_notes, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("incomplete XML in speaker notes"), "{error}");

    let invalid_rels = directory.join("invalid-rels.pptx");
    write_pptx(
        &invalid_rels,
        &[
            ("ppt/slides/slide1.xml", slide.as_bytes()),
            ("ppt/slides/_rels/slide1.xml.rels", b""),
        ],
    );
    let error = execute(&invalid_rels, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("incomplete XML in slide relationships"),
        "{error}"
    );

    let invalid_core = directory.join("invalid-core.pptx");
    write_pptx(
        &invalid_core,
        &[
            ("ppt/slides/slide1.xml", slide.as_bytes()),
            ("docProps/core.xml", b""),
        ],
    );
    let error = execute(
        &invalid_core,
        serde_json::json!({ "metadata_key": "metadata" }),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("incomplete XML in docProps/core.xml"),
        "{error}"
    );
}

async fn reject_corrupt_optional_part(directory: &Path, slide: &str) {
    let path = directory.join("crc-error.pptx");
    let notes = b"<notes><t>CRC-SENTINEL</t></notes>";
    write_pptx(
        &path,
        &[
            ("ppt/slides/slide1.xml", slide.as_bytes()),
            ("ppt/notesSlides/notesSlide1.xml", notes),
        ],
    );
    corrupt_stored_payload(&path, b"CRC-SENTINEL");
    let error = execute(&path, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("cannot decode archive part"), "{error}");
}

#[cfg(unix)]
async fn reject_special_inputs(directory: &Path, target: &Path) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;

    let link = directory.join("linked.pptx");
    symlink(target, &link).unwrap();
    let error = execute(&link, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("failed to open file"), "{error}");

    let fifo = directory.join("input.pptx.pipe");
    let fifo_path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        execute(&fifo, serde_json::json!({})),
    )
    .await
    .expect("extract_pptx must reject a FIFO without blocking");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("not a regular file"), "{error}");
}
