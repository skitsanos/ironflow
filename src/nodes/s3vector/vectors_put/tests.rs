use super::*;

fn vectors() -> serde_json::Value {
    serde_json::json!([{ "key": "doc-1", "data": [0.1, 0.2, 0.3] }])
}

#[test]
fn put_vectors_names_input_sets_only_name_form() {
    let request = prepare_put_vectors_input(
        &serde_json::json!({
            "vector_bucket_name": "demo-bucket",
            "index_name": "demo-index",
            "vectors": vectors()
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
fn put_vectors_arn_input_sets_only_arn_form() {
    let arn = "arn:aws:s3vectors:us-east-1:123456789012:bucket/demo/index/demo";
    let request = prepare_put_vectors_input(
        &serde_json::json!({ "index_arn": arn, "vectors": vectors() }),
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
