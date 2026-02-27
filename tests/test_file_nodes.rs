//! Tests for file operation nodes: copy_file, move_file, delete_file, list_directory.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

// --- copy_file ---

#[tokio::test]
async fn copy_file_happy_path() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("source.txt");
    let dst = dir.path().join("copied.txt");
    std::fs::write(&src, "hello copy").unwrap();

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("copy_file").unwrap();

    let config = serde_json::json!({
        "source": src.to_str().unwrap(),
        "destination": dst.to_str().unwrap(),
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        result.get("copy_file_success").unwrap(),
        &serde_json::Value::Bool(true)
    );
    assert_eq!(
        result
            .get("copy_file_destination")
            .unwrap()
            .as_str()
            .unwrap(),
        dst.to_str().unwrap()
    );
    // Source still exists
    assert!(src.exists());
    // Destination was created with same content
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "hello copy");
}

#[tokio::test]
async fn copy_file_missing_source() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("nonexistent.txt");
    let dst = dir.path().join("copied.txt");

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("copy_file").unwrap();

    let config = serde_json::json!({
        "source": src.to_str().unwrap(),
        "destination": dst.to_str().unwrap(),
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn copy_file_missing_config_param() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("copy_file").unwrap();

    // Missing destination
    let config = serde_json::json!({ "source": "/tmp/x.txt" });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("destination"));
}

#[tokio::test]
async fn copy_file_interpolates_context() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("data.txt");
    let dst = dir.path().join("data_copy.txt");
    std::fs::write(&src, "ctx interpolation").unwrap();

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("copy_file").unwrap();

    let config = serde_json::json!({
        "source": "${ctx.src_path}",
        "destination": "${ctx.dst_path}",
    });

    let ctx = ctx_with(vec![
        (
            "src_path",
            serde_json::Value::String(src.to_str().unwrap().to_string()),
        ),
        (
            "dst_path",
            serde_json::Value::String(dst.to_str().unwrap().to_string()),
        ),
    ]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("copy_file_success").unwrap(),
        &serde_json::Value::Bool(true)
    );
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "ctx interpolation");
}

// --- move_file ---

#[tokio::test]
async fn move_file_happy_path() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("original.txt");
    let dst = dir.path().join("moved.txt");
    std::fs::write(&src, "hello move").unwrap();

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("move_file").unwrap();

    let config = serde_json::json!({
        "source": src.to_str().unwrap(),
        "destination": dst.to_str().unwrap(),
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        result.get("move_file_success").unwrap(),
        &serde_json::Value::Bool(true)
    );
    assert_eq!(
        result
            .get("move_file_destination")
            .unwrap()
            .as_str()
            .unwrap(),
        dst.to_str().unwrap()
    );
    // Source no longer exists
    assert!(!src.exists());
    // Destination has the content
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "hello move");
}

#[tokio::test]
async fn move_file_missing_source() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("nonexistent.txt");
    let dst = dir.path().join("moved.txt");

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("move_file").unwrap();

    let config = serde_json::json!({
        "source": src.to_str().unwrap(),
        "destination": dst.to_str().unwrap(),
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn move_file_missing_config_param() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("move_file").unwrap();

    // Missing source
    let config = serde_json::json!({ "destination": "/tmp/x.txt" });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("source"));
}

// --- delete_file ---

#[tokio::test]
async fn delete_file_happy_path() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("to_delete.txt");
    std::fs::write(&file, "delete me").unwrap();
    assert!(file.exists());

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("delete_file").unwrap();

    let config = serde_json::json!({
        "path": file.to_str().unwrap(),
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        result.get("delete_file_success").unwrap(),
        &serde_json::Value::Bool(true)
    );
    assert_eq!(
        result.get("delete_file_path").unwrap().as_str().unwrap(),
        file.to_str().unwrap()
    );
    assert!(!file.exists());
}

#[tokio::test]
async fn delete_file_nonexistent() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("nonexistent.txt");

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("delete_file").unwrap();

    let config = serde_json::json!({
        "path": file.to_str().unwrap(),
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn delete_file_missing_config_param() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("delete_file").unwrap();

    let config = serde_json::json!({});
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("path"));
}

// --- list_directory ---

#[tokio::test]
async fn list_directory_happy_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "aaa").unwrap();
    std::fs::write(dir.path().join("b.txt"), "bbb").unwrap();
    std::fs::create_dir(dir.path().join("subdir")).unwrap();

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("list_directory").unwrap();

    let config = serde_json::json!({
        "path": dir.path().to_str().unwrap(),
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let files = result.get("files").unwrap().as_array().unwrap();
    assert_eq!(files.len(), 3);

    let names: Vec<&str> = files
        .iter()
        .map(|e| e.get("name").unwrap().as_str().unwrap())
        .collect();
    assert!(names.contains(&"a.txt"));
    assert!(names.contains(&"b.txt"));
    assert!(names.contains(&"subdir"));

    // Check type field
    let subdir_entry = files
        .iter()
        .find(|e| e.get("name").unwrap() == "subdir")
        .unwrap();
    assert_eq!(
        subdir_entry.get("type").unwrap().as_str().unwrap(),
        "directory"
    );

    let file_entry = files
        .iter()
        .find(|e| e.get("name").unwrap() == "a.txt")
        .unwrap();
    assert_eq!(file_entry.get("type").unwrap().as_str().unwrap(), "file");
}

#[tokio::test]
async fn list_directory_nonexistent() {
    let dir = tempfile::tempdir().unwrap();
    let bad_path = dir.path().join("nonexistent_dir");

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("list_directory").unwrap();

    let config = serde_json::json!({
        "path": bad_path.to_str().unwrap(),
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn list_directory_custom_output_key() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "data").unwrap();

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("list_directory").unwrap();

    let config = serde_json::json!({
        "path": dir.path().to_str().unwrap(),
        "output_key": "entries",
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(result.get("entries").is_some());
    assert!(result.get("files").is_none());
    assert_eq!(result.get("entries").unwrap().as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn list_directory_recursive() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("root.txt"), "r").unwrap();
    let sub = dir.path().join("child");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("nested.txt"), "n").unwrap();

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("list_directory").unwrap();

    let config = serde_json::json!({
        "path": dir.path().to_str().unwrap(),
        "recursive": true,
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let files = result.get("files").unwrap().as_array().unwrap();
    // Should have: root.txt, child (dir), nested.txt (from recursive)
    assert_eq!(files.len(), 3);

    let names: Vec<&str> = files
        .iter()
        .map(|e| e.get("name").unwrap().as_str().unwrap())
        .collect();
    assert!(names.contains(&"root.txt"));
    assert!(names.contains(&"child"));
    assert!(names.contains(&"nested.txt"));
}

#[tokio::test]
async fn list_directory_empty() {
    let dir = tempfile::tempdir().unwrap();

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("list_directory").unwrap();

    let config = serde_json::json!({
        "path": dir.path().to_str().unwrap(),
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let files = result.get("files").unwrap().as_array().unwrap();
    assert!(files.is_empty());
}

#[tokio::test]
async fn list_directory_missing_config_param() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("list_directory").unwrap();

    let config = serde_json::json!({});
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("path"));
}
