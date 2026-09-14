#![cfg(unix)]

use base64::Engine;
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;
use std::io::{Cursor, Write};
use std::os::unix::fs::symlink;

fn document() -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    archive.write_all(br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Alias fixture</w:t></w:r></w:p></w:body></w:document>"#).unwrap();
    archive.finish().unwrap().into_inner()
}

#[tokio::test]
async fn directory_aliases_work_for_write_decode_read_extract_and_zip() {
    let directory = tempfile::tempdir().unwrap();
    let real = directory.path().join("real");
    let alias = directory.path().join("alias");
    std::fs::create_dir(&real).unwrap();
    symlink(&real, &alias).unwrap();
    let registry = NodeRegistry::with_builtins();
    let bytes = document();
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    for parent in [&alias, &alias.join("new/nested")] {
        let destination = parent.join("written.docx");
        registry
            .get("write_file")
            .unwrap()
            .execute(
                &json!({"path": destination, "content": encoded, "encoding": "base64"}),
                &Context::new(),
            )
            .await
            .unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        let decoded = parent.join("decoded.docx");
        registry
            .get("base64_decode")
            .unwrap()
            .execute(
                &json!({"output_file": decoded, "input": encoded}),
                &Context::new(),
            )
            .await
            .unwrap();
        for file in [destination, decoded] {
            registry
                .get("read_file")
                .unwrap()
                .execute(
                    &json!({"path": file, "encoding": "base64"}),
                    &Context::new(),
                )
                .await
                .unwrap();
            let output = registry
                .get("extract_word")
                .unwrap()
                .execute(&json!({"path": file}), &Context::new())
                .await
                .unwrap();
            assert!(
                serde_json::to_string(&output)
                    .unwrap()
                    .contains("Alias fixture")
            );
        }
    }
    let archive = directory.path().join("source.zip");
    std::fs::write(&archive, bytes).unwrap();
    registry
        .get("zip_extract")
        .unwrap()
        .execute(
            &json!({"path": archive, "destination": alias}),
            &Context::new(),
        )
        .await
        .unwrap();
    assert!(real.join("word/document.xml").is_file());

    let appended = alias.join("append.txt");
    for content in ["first", "second"] {
        registry
            .get("write_file")
            .unwrap()
            .execute(
                &json!({"path": appended, "content": content, "append": true}),
                &Context::new(),
            )
            .await
            .unwrap();
    }
    assert_eq!(std::fs::read(appended).unwrap(), b"firstsecond");
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn macos_tmp_alias_supports_direct_file_writes() {
    let directory = tempfile::Builder::new()
        .prefix("ironflow-if134-")
        .tempdir_in("/tmp")
        .unwrap();
    let path = std::path::Path::new("/tmp").join(format!(
        "{}.txt",
        directory.path().file_name().unwrap().to_str().unwrap()
    ));
    let result = NodeRegistry::with_builtins()
        .get("write_file")
        .unwrap()
        .execute(&json!({"path": path, "content": "alias"}), &Context::new())
        .await;
    let contents = std::fs::read(&path);
    let _ = std::fs::remove_file(&path);
    result.unwrap();
    assert_eq!(contents.unwrap(), b"alias");
}
