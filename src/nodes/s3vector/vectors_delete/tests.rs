use super::*;

#[test]
fn delete_vectors_names_input_sets_only_name_form() {
    let request = prepare_delete_vectors_input(
        &serde_json::json!({
            "vector_bucket_name": "demo-bucket",
            "index_name": "demo-index",
            "keys": ["doc-1"]
        }),
        &Context::new(),
    )
    .unwrap()
    .request
    .build()
    .unwrap();

    assert_eq!(request.vector_bucket_name(), Some("demo-bucket"));
    assert_eq!(request.index_name(), Some("demo-index"));
    assert_eq!(request.index_arn(), None);
}

#[test]
fn delete_vectors_arn_input_sets_only_arn_form() {
    let arn = "arn:aws:s3vectors:us-east-1:123456789012:bucket/demo/index/demo";
    let request = prepare_delete_vectors_input(
        &serde_json::json!({ "index_arn": arn, "keys": ["doc-1"] }),
        &Context::new(),
    )
    .unwrap()
    .request
    .build()
    .unwrap();

    assert_eq!(request.vector_bucket_name(), None);
    assert_eq!(request.index_name(), None);
    assert_eq!(request.index_arn(), Some(arn));
}

#[test]
fn delete_vectors_rejects_missing_explicit_target_before_keys() {
    let error =
        prepare_delete_vectors_input(&serde_json::json!({ "keys": ["doc-1"] }), &Context::new())
            .err()
            .unwrap();

    assert_eq!(
        error.to_string(),
        "s3vector_delete_vectors requires 'index_name' or 'index_arn'"
    );
}
