#![cfg(unix)]

use std::ffi::CString;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, symlink};
use std::path::Path;
use std::time::Duration;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    for (name, contents) in entries {
        archive.start_file(*name, options).unwrap();
        archive.write_all(contents).unwrap();
    }
    archive.finish().unwrap();
}

#[tokio::test]
async fn zip_extract_rejects_parent_symlink_without_touching_external_file() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let zip_path = directory.path().join("archive.zip");
    let destination = directory.path().join("destination");
    let outside_file = outside.path().join("escape.txt");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(&outside_file, b"outside sentinel").unwrap();
    symlink(outside.path(), destination.join("linked")).unwrap();
    write_zip(&zip_path, &[("linked/escape.txt", b"archive payload")]);

    let node = NodeRegistry::with_builtins().get("zip_extract").unwrap();
    let result = node
        .execute(
            &serde_json::json!({
                "path": zip_path.to_str().unwrap(),
                "destination": destination.to_str().unwrap(),
                "overwrite": true,
            }),
            &Context::new(),
        )
        .await;

    assert!(
        result.is_err(),
        "a destination parent symlink must be rejected"
    );
    assert_eq!(std::fs::read(&outside_file).unwrap(), b"outside sentinel");
    assert!(
        std::fs::symlink_metadata(destination.join("linked"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[tokio::test]
async fn zip_extract_rejects_leaf_symlink_without_truncating_target() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let zip_path = directory.path().join("archive.zip");
    let destination = directory.path().join("destination");
    let outside_file = outside.path().join("target.txt");
    let destination_leaf = destination.join("target.txt");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(&outside_file, b"outside sentinel").unwrap();
    symlink(&outside_file, &destination_leaf).unwrap();
    write_zip(&zip_path, &[("target.txt", b"archive payload")]);

    let node = NodeRegistry::with_builtins().get("zip_extract").unwrap();
    let result = node
        .execute(
            &serde_json::json!({
                "path": zip_path.to_str().unwrap(),
                "destination": destination.to_str().unwrap(),
                "overwrite": true,
            }),
            &Context::new(),
        )
        .await;

    assert!(
        result.is_err(),
        "a destination leaf symlink must be rejected"
    );
    assert_eq!(std::fs::read(&outside_file).unwrap(), b"outside sentinel");
    assert!(
        std::fs::symlink_metadata(&destination_leaf)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[tokio::test]
async fn zip_extract_rejects_fifo_leaf_without_blocking() {
    let directory = tempfile::tempdir().unwrap();
    let zip_path = directory.path().join("archive.zip");
    let destination = directory.path().join("destination");
    let fifo = destination.join("payload.pipe");
    std::fs::create_dir(&destination).unwrap();
    let fifo_name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    let result = unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) };
    assert_eq!(
        result,
        0,
        "mkfifo failed: {}",
        std::io::Error::last_os_error()
    );
    write_zip(&zip_path, &[("payload.pipe", b"archive payload")]);

    let node = NodeRegistry::with_builtins().get("zip_extract").unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        node.execute(
            &serde_json::json!({
                "path": zip_path.to_str().unwrap(),
                "destination": destination.to_str().unwrap(),
                "overwrite": true,
            }),
            &Context::new(),
        ),
    )
    .await
    .expect("zip_extract blocked while inspecting a FIFO destination leaf");

    assert!(result.is_err(), "a FIFO destination leaf must be rejected");
    assert!(
        std::fs::symlink_metadata(&fifo)
            .unwrap()
            .file_type()
            .is_fifo()
    );
}

#[tokio::test]
async fn zip_create_rejects_external_directory_symlink() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let zip_path = directory.path().join("archive.zip");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(outside.path().join("secret.txt"), b"outside secret").unwrap();
    symlink(outside.path(), source.join("external")).unwrap();

    let node = NodeRegistry::with_builtins().get("zip_create").unwrap();
    let result = node
        .execute(
            &serde_json::json!({
                "source": source.to_str().unwrap(),
                "zip_path": zip_path.to_str().unwrap(),
            }),
            &Context::new(),
        )
        .await;

    let error = result.expect_err("a source-tree symlink must be rejected");
    assert!(error.to_string().contains("symlink"), "{error:#}");
    assert!(
        !zip_path.exists(),
        "a rejected source must not publish an archive"
    );
}

#[tokio::test]
async fn zip_create_rejects_source_root_symlink() {
    let directory = tempfile::tempdir().unwrap();
    let source_file = directory.path().join("source.txt");
    let source_link = directory.path().join("source-link.txt");
    let zip_path = directory.path().join("archive.zip");
    std::fs::write(&source_file, b"source").unwrap();
    symlink(&source_file, &source_link).unwrap();

    let node = NodeRegistry::with_builtins().get("zip_create").unwrap();
    let result = node
        .execute(
            &serde_json::json!({
                "source": source_link.to_str().unwrap(),
                "zip_path": zip_path.to_str().unwrap(),
            }),
            &Context::new(),
        )
        .await;

    let error = result.expect_err("a source-root symlink must be rejected");
    assert!(error.to_string().contains("symlink"), "{error:#}");
    assert!(!zip_path.exists());
}

#[tokio::test]
async fn zip_create_rejects_symlink_cycle_promptly() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let zip_path = directory.path().join("archive.zip");
    std::fs::create_dir(&source).unwrap();
    symlink(".", source.join("cycle")).unwrap();

    let node = NodeRegistry::with_builtins().get("zip_create").unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        node.execute(
            &serde_json::json!({
                "source": source.to_str().unwrap(),
                "zip_path": zip_path.to_str().unwrap(),
            }),
            &Context::new(),
        ),
    )
    .await
    .expect("zip_create followed a source symlink cycle instead of rejecting it");

    let error = result.expect_err("a source-tree symlink cycle must be rejected");
    assert!(error.to_string().contains("symlink"), "{error:#}");
    assert!(
        !zip_path.exists(),
        "a rejected source must not publish an archive"
    );
}

#[tokio::test]
async fn zip_create_rejects_output_symlink_without_truncating_target() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.txt");
    let zip_path = directory.path().join("archive.zip");
    let outside_file = outside.path().join("target.zip");
    std::fs::write(&source, b"archive source").unwrap();
    std::fs::write(&outside_file, b"outside sentinel").unwrap();
    symlink(&outside_file, &zip_path).unwrap();

    let node = NodeRegistry::with_builtins().get("zip_create").unwrap();
    let result = node
        .execute(
            &serde_json::json!({
                "source": source.to_str().unwrap(),
                "zip_path": zip_path.to_str().unwrap(),
            }),
            &Context::new(),
        )
        .await;

    assert!(result.is_err(), "a ZIP output symlink must be rejected");
    assert_eq!(std::fs::read(&outside_file).unwrap(), b"outside sentinel");
    assert!(
        std::fs::symlink_metadata(&zip_path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[tokio::test]
async fn zip_read_nodes_reject_archive_symlink() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zip");
    let archive_link = directory.path().join("archive-link.zip");
    let destination = directory.path().join("destination");
    write_zip(&archive, &[("entry.txt", b"payload")]);
    symlink(&archive, &archive_link).unwrap();

    let registry = NodeRegistry::with_builtins();
    for node_type in ["zip_list", "zip_extract"] {
        let node = registry.get(node_type).unwrap();
        let config = if node_type == "zip_list" {
            serde_json::json!({ "path": archive_link.to_str().unwrap() })
        } else {
            serde_json::json!({
                "path": archive_link.to_str().unwrap(),
                "destination": destination.to_str().unwrap(),
            })
        };
        let error = node
            .execute(&config, &Context::new())
            .await
            .expect_err("a final archive symlink must be rejected");
        assert!(
            error.to_string().contains("failed to open file"),
            "{error:#}"
        );
    }
    assert!(!destination.exists());
}
