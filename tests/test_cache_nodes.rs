//! Tests for cache_set and cache_get nodes.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

// --- Helpers ---

fn empty_ctx() -> Context {
    HashMap::new()
}

// --- cache_set: memory backend ---

#[tokio::test]
async fn cache_set_memory_happy_path() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("cache_set").expect("cache_set node exists");

    let config = serde_json::json!({
        "key": "test_set_memory_key",
        "value": "hello world",
        "backend": "memory"
    });

    let output = node
        .execute(&config, empty_ctx())
        .await
        .expect("cache_set succeeds");

    assert_eq!(
        output.get("cache_key").unwrap(),
        &serde_json::json!("test_set_memory_key")
    );
    assert_eq!(
        output.get("cache_stored").unwrap(),
        &serde_json::json!(true)
    );
}

// --- cache_get: memory backend (set then get) ---

#[tokio::test]
async fn cache_get_memory_happy_path() {
    let reg = NodeRegistry::with_builtins();
    let set_node = reg.get("cache_set").expect("cache_set node exists");
    let get_node = reg.get("cache_get").expect("cache_get node exists");

    // Use a unique key to avoid interference from other tests
    let key = "test_get_memory_key";

    let set_config = serde_json::json!({
        "key": key,
        "value": 42,
        "backend": "memory"
    });
    set_node
        .execute(&set_config, empty_ctx())
        .await
        .expect("cache_set succeeds");

    let get_config = serde_json::json!({
        "key": key,
        "backend": "memory"
    });
    let output = get_node
        .execute(&get_config, empty_ctx())
        .await
        .expect("cache_get succeeds");

    assert_eq!(output.get("cached_value").unwrap(), &serde_json::json!(42));
    assert_eq!(output.get("cache_hit").unwrap(), &serde_json::json!(true));
}

// --- cache_get: missing key ---

#[tokio::test]
async fn cache_get_memory_missing_key() {
    let reg = NodeRegistry::with_builtins();
    let get_node = reg.get("cache_get").expect("cache_get node exists");

    let config = serde_json::json!({
        "key": "nonexistent_key_12345",
        "backend": "memory"
    });
    let output = get_node
        .execute(&config, empty_ctx())
        .await
        .expect("cache_get succeeds");

    assert_eq!(
        output.get("cached_value").unwrap(),
        &serde_json::Value::Null
    );
    assert_eq!(output.get("cache_hit").unwrap(), &serde_json::json!(false));
}

// --- cache_set: file backend ---

#[tokio::test]
async fn cache_set_file_backend() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("cache_set").expect("cache_set node exists");

    let tmp = tempfile::tempdir().expect("create tempdir");
    let cache_dir = tmp.path().to_str().unwrap();

    let config = serde_json::json!({
        "key": "file_test_key",
        "value": {"name": "ironflow"},
        "backend": "file",
        "cache_dir": cache_dir
    });

    let output = node
        .execute(&config, empty_ctx())
        .await
        .expect("cache_set file succeeds");

    assert_eq!(
        output.get("cache_key").unwrap(),
        &serde_json::json!("file_test_key")
    );
    assert_eq!(
        output.get("cache_stored").unwrap(),
        &serde_json::json!(true)
    );

    // Verify the file was actually created
    let cache_file = tmp.path().join("file_test_key.json");
    assert!(cache_file.exists(), "cache file should exist on disk");
}

// --- cache_get: file backend (set then get) ---

#[tokio::test]
async fn cache_get_file_backend() {
    let reg = NodeRegistry::with_builtins();
    let set_node = reg.get("cache_set").expect("cache_set node exists");
    let get_node = reg.get("cache_get").expect("cache_get node exists");

    let tmp = tempfile::tempdir().expect("create tempdir");
    let cache_dir = tmp.path().to_str().unwrap();

    let set_config = serde_json::json!({
        "key": "file_roundtrip",
        "value": [1, 2, 3],
        "backend": "file",
        "cache_dir": cache_dir
    });
    set_node
        .execute(&set_config, empty_ctx())
        .await
        .expect("cache_set file succeeds");

    let get_config = serde_json::json!({
        "key": "file_roundtrip",
        "backend": "file",
        "cache_dir": cache_dir
    });
    let output = get_node
        .execute(&get_config, empty_ctx())
        .await
        .expect("cache_get file succeeds");

    assert_eq!(
        output.get("cached_value").unwrap(),
        &serde_json::json!([1, 2, 3])
    );
    assert_eq!(output.get("cache_hit").unwrap(), &serde_json::json!(true));
}
