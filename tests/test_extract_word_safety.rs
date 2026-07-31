//! Focused IF-065 regressions for `extract_word`.
//!
//! Environment mutations are kept in one sequential test so extraction limits
//! cannot race another test in this process.

use std::io::Write;
use std::path::{Path, PathBuf};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use ironflow::util::execution::with_execution_deadline;

struct EnvGuard {
    values: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    fn new(keys: &[&'static str]) -> Self {
        Self {
            values: keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        }
    }

    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) {
        // This binary contains one test, so its process-global limits are isolated.
        unsafe { std::env::set_var(key, value) };
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.values {
            unsafe {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }
}

fn document(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>{body}</w:body></w:document>"#
    )
}

fn write_docx(directory: &Path, name: &str, entries: &[(&str, &str)]) -> PathBuf {
    let path = directory.join(name);
    let file = std::fs::File::create(&path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    for (entry_name, body) in entries {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file(*entry_name, options).unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
    path
}

async fn execute(path: &Path, extra: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let mut config = serde_json::json!({ "path": path });
    config.as_object_mut().unwrap().extend(
        extra
            .as_object()
            .expect("test config extension must be an object")
            .clone(),
    );
    Ok(serde_json::to_value(
        node.execute(&config, &Context::new()).await?,
    )?)
}

#[tokio::test(flavor = "current_thread")]
async fn extract_word_is_strict_bounded_and_special_file_safe() {
    let _env = EnvGuard::new(&[
        "IRONFLOW_MAX_EXTRACT_ITEMS",
        "IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES",
        "IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES",
    ]);
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_ITEMS", "100000");
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "1048576");
    EnvGuard::set("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES", "1048576");

    let directory = tempfile::tempdir().unwrap();
    let valid_xml = document("<w:p><w:r><w:t>Hello DOCX</w:t></w:r></w:p>");
    let valid = write_docx(
        directory.path(),
        "valid.docx",
        &[("word/document.xml", &valid_xml)],
    );

    let output = execute(
        &valid,
        serde_json::json!({
            "output_key": "body",
            "metadata_key": "metadata",
            "comments_key": "comments"
        }),
    )
    .await
    .unwrap();
    assert_eq!(output["body"], "Hello DOCX");
    assert_eq!(output["metadata"], serde_json::json!({}));
    assert_eq!(output["comments"], serde_json::json!([]));

    for (key, invalid) in [
        ("format", serde_json::json!(42)),
        ("output_key", serde_json::json!(false)),
        ("metadata_key", serde_json::json!([])),
        ("comments_key", serde_json::json!({})),
    ] {
        let extra = serde_json::Value::Object([(key.to_string(), invalid)].into_iter().collect());
        let error = execute(&valid, extra).await.unwrap_err().to_string();
        assert!(
            error.contains(&format!("'{key}' must be a string")),
            "{error}"
        );
    }

    for collision in [
        serde_json::json!({ "output_key": "same", "metadata_key": "same" }),
        serde_json::json!({ "metadata_key": "same", "comments_key": "same" }),
    ] {
        let error = execute(&valid, collision).await.unwrap_err().to_string();
        assert!(
            error.contains("must name different context keys"),
            "{error}"
        );
    }

    let malformed_numbering =
        r#"<w:numbering xmlns:w="urn:test"><w:num w:numId=broken/></w:numbering>"#;
    let malformed = write_docx(
        directory.path(),
        "malformed-optional.docx",
        &[
            ("word/document.xml", &valid_xml),
            ("word/numbering.xml", malformed_numbering),
        ],
    );
    let error = execute(&malformed, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("word/numbering.xml"), "{error}");

    let nested_comments = r#"<w:comments xmlns:w="urn:test">
<w:comment w:id="1"><w:comment w:id="2"></w:comment></w:comment>
</w:comments>"#;
    let nested = write_docx(
        directory.path(),
        "nested-comments.docx",
        &[
            ("word/document.xml", &valid_xml),
            ("word/comments.xml", nested_comments),
        ],
    );
    let error = execute(&nested, serde_json::json!({ "comments_key": "comments" }))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("nested comments"), "{error}");

    let numbering = r#"<w:numbering xmlns:w="urn:test">
<w:abstractNum w:abstractNumId="1"><w:lvl><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum>
<w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#;
    let cumulative = write_docx(
        directory.path(),
        "cumulative.docx",
        &[
            ("word/document.xml", &valid_xml),
            ("word/numbering.xml", numbering),
        ],
    );
    let cumulative_limit = valid_xml.len().max(numbering.len()) + 1;
    EnvGuard::set(
        "IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES",
        cumulative_limit.to_string(),
    );
    let error = execute(&cumulative, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("declared uncompressed"), "{error}");
    EnvGuard::set("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES", "1048576");

    EnvGuard::set("IRONFLOW_MAX_EXTRACT_ITEMS", "5");
    let error = execute(&valid, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("IRONFLOW_MAX_EXTRACT_ITEMS"), "{error}");
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_ITEMS", "100000");

    let range_starts = (0..12)
        .map(|id| format!(r#"<w:commentRangeStart w:id="{id}"/>"#))
        .collect::<String>();
    let range_ends = (0..12)
        .map(|id| format!(r#"<w:commentRangeEnd w:id="{id}"/>"#))
        .collect::<String>();
    let anchored_text = "bounded ".repeat(50);
    let amplified_document = document(&format!(
        "<w:p>{range_starts}<w:r><w:t>{anchored_text}</w:t></w:r>{range_ends}</w:p>"
    ));
    let comments = format!(
        "<w:comments xmlns:w=\"urn:test\">{}</w:comments>",
        (0..12)
            .map(|id| format!(
                r#"<w:comment w:id="{id}"><w:p><w:r><w:t>note</w:t></w:r></w:p></w:comment>"#
            ))
            .collect::<String>()
    );
    let amplified = write_docx(
        directory.path(),
        "amplified-comments.docx",
        &[
            ("word/document.xml", &amplified_document),
            ("word/comments.xml", &comments),
        ],
    );
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "4096");
    let error = execute(
        &amplified,
        serde_json::json!({ "comments_key": "comments" }),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"),
        "{error}"
    );

    // The source XML and plain result both fit independently. JSON retains a
    // second run-text copy plus the concatenated paragraph text, so the
    // formatting phase must reject that transient amplification before clone.
    let formatting_text = "x".repeat(250);
    let formatting_xml = document(&format!(
        "<w:p><w:r><w:t>{formatting_text}</w:t></w:r></w:p>"
    ));
    let formatting = write_docx(
        directory.path(),
        "formatting-amplification.docx",
        &[("word/document.xml", &formatting_xml)],
    );
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "550");
    let plain = execute(&formatting, serde_json::json!({})).await.unwrap();
    assert_eq!(plain["content"], formatting_text);
    let error = execute(&formatting, serde_json::json!({ "format": "json" }))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"),
        "{error}"
    );

    // A large list level has a compact XML representation but expands into
    // Markdown indentation. Charge that generated wrapping before allocating.
    let list_xml = document(
        r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="1000"/></w:numPr></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#,
    );
    let list = write_docx(
        directory.path(),
        "markdown-amplification.docx",
        &[("word/document.xml", &list_xml)],
    );
    let plain = execute(&list, serde_json::json!({})).await.unwrap();
    assert_eq!(plain["content"], "x");
    let error = execute(&list, serde_json::json!({ "format": "markdown" }))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"),
        "{error}"
    );
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "1048576");

    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let config = serde_json::json!({ "path": valid.to_string_lossy() });
    let error = with_execution_deadline(
        Some(tokio::time::Instant::now()),
        node.execute(&config, &Context::new()),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("deadline exceeded"), "{error}");

    #[cfg(unix)]
    reject_special_inputs_without_blocking(directory.path(), &valid).await;
}

#[cfg(unix)]
async fn reject_special_inputs_without_blocking(directory: &Path, valid: &Path) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;

    let link = directory.join("linked.docx");
    symlink(valid, &link).unwrap();
    let error = execute(&link, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("failed to open file"), "{error}");

    let fifo = directory.join("input.docx");
    let fifo_path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        execute(&fifo, serde_json::json!({})),
    )
    .await
    .expect("extract_word blocked while opening a FIFO");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("not a regular file"), "{error}");
}
