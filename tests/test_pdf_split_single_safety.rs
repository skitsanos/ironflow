//! IF-218 (a): single-page `pdf_split` publishes through the staged writer.
#![cfg(unix)]

#[path = "pdf_groups/fixture.rs"]
mod fixture;

use std::os::unix::fs::{MetadataExt, PermissionsExt};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use lopdf::Document;
use serde_json::json;

async fn split(
    source: &std::path::Path,
    output: &std::path::Path,
    pages: &str,
) -> anyhow::Result<ironflow::engine::types::NodeOutput> {
    NodeRegistry::with_builtins()
        .get("pdf_split")
        .unwrap()
        .execute(
            &json!({"path": source, "output_dir": output, "pages": pages}),
            &Context::new(),
        )
        .await
}

#[tokio::test]
async fn a_symlinked_page_destination_is_refused_and_its_target_is_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(2).save(&source).unwrap();
    let output = directory.path().join("pages");
    std::fs::create_dir(&output).unwrap();
    let victim = directory.path().join("victim.txt");
    std::fs::write(&victim, b"keep me").unwrap();
    std::os::unix::fs::symlink(&victim, output.join("source_2.pdf")).unwrap();

    let error = format!("{:#}", split(&source, &output, "all").await.unwrap_err());

    assert!(error.contains("symlink"), "{error}");
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep me");
    assert!(
        std::fs::symlink_metadata(output.join("source_2.pdf"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    // Page 1 was published before the refusal; nothing else is left behind.
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 2);
}

#[tokio::test]
async fn a_dangling_symlink_destination_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(1).save(&source).unwrap();
    let output = directory.path().join("pages");
    std::fs::create_dir(&output).unwrap();
    let missing = directory.path().join("missing.pdf");
    std::os::unix::fs::symlink(&missing, output.join("source_1.pdf")).unwrap();

    let error = format!("{:#}", split(&source, &output, "all").await.unwrap_err());

    assert!(error.contains("symlink"), "{error}");
    assert!(!missing.exists());
}

#[tokio::test]
async fn an_existing_page_file_is_replaced_atomically_and_keeps_its_mode() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(1).save(&source).unwrap();
    let output = directory.path().join("pages");
    std::fs::create_dir(&output).unwrap();
    let existing = output.join("source_1.pdf");
    std::fs::write(&existing, b"old").unwrap();
    std::fs::set_permissions(&existing, std::fs::Permissions::from_mode(0o640)).unwrap();
    let before = std::fs::metadata(&existing).unwrap().ino();

    split(&source, &output, "all").await.unwrap();

    let after = std::fs::metadata(&existing).unwrap();
    // A replacement is a new inode renamed over the old name, never an
    // in-place truncation that a reader could observe half written.
    assert_ne!(after.ino(), before);
    assert_eq!(after.permissions().mode() & 0o777, 0o640);
    assert_eq!(Document::load(&existing).unwrap().get_pages().len(), 1);
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 1);
}

#[tokio::test]
async fn repeated_selections_and_missing_output_directories_still_work() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(2).save(&source).unwrap();
    let output = directory.path().join("nested").join("pages");

    let result = split(&source, &output, "2,1,2").await.unwrap();

    let files = result["pdf_split_files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert_eq!(files[0], files[2]);
    assert_eq!(result["pdf_split_page_count"], 3);
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 2);
    for file in files {
        let pages = Document::load(file.as_str().unwrap()).unwrap().get_pages();
        assert_eq!(pages.len(), 1);
    }
}
