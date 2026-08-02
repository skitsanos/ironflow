use std::io::Write;
use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use ironflow::util::execution::with_execution_deadline;
use tokio::time::Instant;

fn write_valid_zip(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file("entry.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"valid zip fixture").unwrap();
    writer.finish().unwrap();
}

async fn execute_with_expired_deadline(
    node_type: &str,
    config: &serde_json::Value,
) -> anyhow::Error {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get(node_type).unwrap();
    let context = Context::new();

    with_execution_deadline(Some(Instant::now()), node.execute(config, &context))
        .await
        .expect_err("an already-expired execution deadline must stop ZIP work")
}

#[tokio::test(flavor = "current_thread")]
async fn zip_create_observes_the_execution_deadline_before_filesystem_work() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.txt");
    let archive = directory.path().join("created.zip");
    std::fs::write(&source, "valid source fixture").unwrap();

    let error = execute_with_expired_deadline(
        "zip_create",
        &serde_json::json!({
            "source": source,
            "zip_path": archive,
        }),
    )
    .await;

    assert!(
        error.to_string().contains("step deadline exceeded"),
        "{error:#}"
    );
    assert!(!archive.exists(), "expired creation wrote an archive");
}

#[tokio::test(flavor = "current_thread")]
async fn zip_list_observes_the_execution_deadline_before_archive_work() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("valid.zip");
    write_valid_zip(&archive);

    let error = execute_with_expired_deadline(
        "zip_list",
        &serde_json::json!({
            "path": archive,
        }),
    )
    .await;

    assert!(
        error.to_string().contains("step deadline exceeded"),
        "{error:#}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn zip_extract_observes_the_execution_deadline_before_filesystem_work() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("valid.zip");
    let destination = directory.path().join("extracted");
    write_valid_zip(&archive);

    let error = execute_with_expired_deadline(
        "zip_extract",
        &serde_json::json!({
            "path": archive,
            "destination": destination,
        }),
    )
    .await;

    assert!(
        error.to_string().contains("step deadline exceeded"),
        "{error:#}"
    );
    assert!(
        !destination.exists(),
        "expired extraction created its destination"
    );
}
