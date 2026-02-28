use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    Context::new()
}

#[tokio::test]
async fn s3vector_nodes_are_registered() {
    let reg = NodeRegistry::with_builtins();
    assert!(reg.get("s3vector_create_bucket").is_some());
    assert!(reg.get("s3vector_get_bucket").is_some());
    assert!(reg.get("s3vector_create_index").is_some());
    assert!(reg.get("s3vector_get_index").is_some());
    assert!(reg.get("s3vector_put_vectors").is_some());
    assert!(reg.get("s3vector_query_vectors").is_some());
    assert!(reg.get("s3vector_delete_vectors").is_some());
}

#[tokio::test]
async fn s3vector_create_bucket_requires_name() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3vector_create_bucket").unwrap();

    let config = serde_json::json!({
        "output_key": "vector_bucket"
    });
    let err = node.execute(&config, empty_ctx()).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("s3vector_create_bucket requires 'vector_bucket_name'"),
        "{}",
        err
    );
}

#[tokio::test]
async fn s3vector_create_index_requires_bucket() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3vector_create_index").unwrap();

    let config = serde_json::json!({
        "index_name": "demo-index",
        "data_type": "float32",
        "distance_metric": "euclidean",
        "dimension": 3
    });
    let err = node.execute(&config, empty_ctx()).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("s3vector_create_index requires 'vector_bucket_name'"),
        "{}",
        err
    );
}

#[tokio::test]
async fn s3vector_get_index_requires_index() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3vector_get_index").unwrap();

    let config = serde_json::json!({
        "vector_bucket_name": "demo-bucket"
    });
    let err = node.execute(&config, empty_ctx()).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("s3vector_get_index requires 'index_name'"),
        "{}",
        err
    );
}

#[tokio::test]
async fn s3vector_put_vectors_requires_index() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3vector_put_vectors").unwrap();

    let config = serde_json::json!({
        "vector_bucket_name": "demo-bucket",
        "vectors": [
            {
                "key": "sample-1",
                "data": [0.1, 0.2, 0.3]
            }
        ]
    });
    let err = node.execute(&config, empty_ctx()).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("s3vector_put_vectors requires 'index_name'"),
        "{}",
        err
    );
}

#[tokio::test]
async fn s3vector_query_vectors_requires_top_k() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3vector_query_vectors").unwrap();

    let config = serde_json::json!({
        "vector_bucket_name": "demo-bucket",
        "index_name": "demo-index",
        "query_vector": [0.1, 0.2, 0.3]
    });
    let err = node.execute(&config, empty_ctx()).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("s3vector_query_vectors requires 'top_k' field"),
        "{}",
        err
    );
}

#[tokio::test]
async fn s3vector_delete_vectors_requires_keys() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("s3vector_delete_vectors").unwrap();

    let config = serde_json::json!({
        "vector_bucket_name": "demo-bucket",
        "index_name": "demo-index"
    });
    let err = node.execute(&config, empty_ctx()).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("s3vector_delete_vectors requires 'keys'"),
        "{}",
        err
    );
}
