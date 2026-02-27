use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    Context::new()
}

#[test]
fn s3_nodes_are_registered() {
    let reg = NodeRegistry::with_builtins();
    assert!(reg.get("s3_presign_url").is_some());
    assert!(reg.get("s3_get_object").is_some());
    assert!(reg.get("s3_put_object").is_some());
    assert!(reg.get("s3_delete_object").is_some());
    assert!(reg.get("s3_copy_object").is_some());
    assert!(reg.get("s3_list_objects").is_some());
    assert!(reg.get("s3_list_buckets").is_some());
}

#[tokio::test]
async fn s3_presign_url_requires_bucket() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3_presign_url").unwrap();

    let config = serde_json::json!({
        "key": "demo.txt",
        "method": "GET"
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("s3_presign_url requires 'bucket'"));
}

#[tokio::test]
async fn s3_presign_url_requires_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3_presign_url").unwrap();

    let config = serde_json::json!({
        "bucket": "test-bucket",
        "method": "GET"
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("s3_presign_url requires 'key'"));
}

#[tokio::test]
async fn s3_presign_url_rejects_invalid_method() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3_presign_url").unwrap();

    let config = serde_json::json!({
        "bucket": "test-bucket",
        "key": "demo.txt",
        "method": "POST"
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("s3_presign_url method 'POST' is not supported"));
}

#[tokio::test]
async fn s3_presign_url_rejects_invalid_expires_in() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3_presign_url").unwrap();

    let config = serde_json::json!({
        "bucket": "test-bucket",
        "key": "demo.txt",
        "expires_in": 0
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("s3_presign_url requires 'expires_in'")
            || error.contains("s3_presign_url invalid expires_in")
    );
}

#[tokio::test]
async fn s3_put_object_requires_bucket() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3_put_object").unwrap();

    let config = serde_json::json!({
        "key": "demo.txt",
        "content": "hello"
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("s3_put_object requires 'bucket'"));
}

#[tokio::test]
async fn s3_put_object_requires_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3_put_object").unwrap();

    let config = serde_json::json!({
        "bucket": "test-bucket",
        "content": "hello"
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("s3_put_object requires 'key'"));
}

#[tokio::test]
async fn s3_put_object_requires_payload() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3_put_object").unwrap();

    let config = serde_json::json!({
        "bucket": "test-bucket",
        "key": "demo.txt"
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("requires one of 'content', 'source_key', or 'source_path'"));
}

#[tokio::test]
async fn s3_get_object_requires_bucket() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3_get_object").unwrap();

    let config = serde_json::json!({
        "key": "demo.txt"
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("s3_get_object requires 'bucket'"));
}

#[tokio::test]
async fn s3_delete_object_requires_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3_delete_object").unwrap();

    let config = serde_json::json!({
        "bucket": "test-bucket"
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("s3_delete_object requires 'key'")
    );
}

#[tokio::test]
async fn s3_copy_object_requires_source_bucket() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3_copy_object").unwrap();

    let config = serde_json::json!({
        "source_key": "a.txt",
        "bucket": "test-bucket",
        "key": "b.txt"
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("s3_copy_object requires 'source_bucket'"));
}
