// IF-036: extract nodes must enforce the IRONFLOW_MAX_* byte limits on the file
// and zip-entry contents they read, so an oversized document (or a zip bomb)
// becomes an ordinary node error instead of an unbounded allocation.
//
// This file is intentionally the only test in its binary: it mutates a
// process-global limit env var, and running alone avoids contaminating other
// tests. The two phases run sequentially within one test for the same reason.

use std::io::Write;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn doc_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>{body}</w:body></w:document>"#
    )
}

fn make_docx(dir: &std::path::Path, document_xml: &str) -> std::path::PathBuf {
    let path = dir.join("bomb.docx");
    let file = std::fs::File::create(&path).unwrap();
    let mut zw = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zw.start_file("word/document.xml", opts).unwrap();
    zw.write_all(document_xml.as_bytes()).unwrap();
    zw.finish().unwrap();
    path
}

#[tokio::test]
async fn extract_word_enforces_zip_uncompressed_limit() {
    let dir = tempfile::tempdir().unwrap();
    // A body that is small compressed but ~40 KB uncompressed.
    let body = "<w:p><w:r><w:t>lorem ipsum dolor sit amet</w:t></w:r></w:p>".repeat(700);
    let path = make_docx(dir.path(), &doc_xml(&body));

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("extract_word").unwrap();
    let config = serde_json::json!({ "path": path.to_str().unwrap() });

    // With a tiny uncompressed cap, the oversized document.xml entry is rejected.
    unsafe {
        std::env::set_var("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES", "1024");
    }
    let rejected = node.execute(&config, &Context::new()).await;
    assert!(
        rejected.is_err(),
        "expected the oversized zip entry to be rejected"
    );
    let message = rejected.unwrap_err().to_string();
    assert!(
        message.contains("limit"),
        "error should reference the byte limit, got: {message}"
    );

    // With a generous cap the same document parses normally, proving the guard
    // only rejects genuinely oversized input.
    unsafe {
        std::env::set_var("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES", "536870912");
    }
    let accepted = node.execute(&config, &Context::new()).await;
    assert!(accepted.is_ok(), "a document within the cap must parse");

    unsafe {
        std::env::remove_var("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES");
    }
}
