use super::*;

fn create_index_config(bucket: serde_json::Value) -> serde_json::Value {
    let mut config = serde_json::json!({
        "index_name": "demo-index",
        "data_type": "float32",
        "distance_metric": "cosine",
        "dimension": 3
    });
    config
        .as_object_mut()
        .unwrap()
        .extend(bucket.as_object().unwrap().clone());
    config
}

#[test]
fn create_index_bucket_name_input_sets_only_name() {
    let config = create_index_config(serde_json::json!({ "vector_bucket_name": "demo-bucket" }));
    let request = prepare_create_index_input(&config, &Context::new())
        .unwrap()
        .request
        .build()
        .unwrap();

    assert_eq!(request.vector_bucket_name(), Some("demo-bucket"));
    assert_eq!(request.vector_bucket_arn(), None);
    assert_eq!(request.index_name(), Some("demo-index"));
}

#[test]
fn create_index_bucket_arn_input_sets_only_arn() {
    let arn = "arn:aws:s3vectors:us-east-1:123456789012:bucket/demo";
    let config = create_index_config(serde_json::json!({ "vector_bucket_arn": arn }));
    let request = prepare_create_index_input(&config, &Context::new())
        .unwrap()
        .request
        .build()
        .unwrap();

    assert_eq!(request.vector_bucket_name(), None);
    assert_eq!(request.vector_bucket_arn(), Some(arn));
    assert_eq!(request.index_name(), Some("demo-index"));
}

#[test]
fn get_index_names_input_sets_only_name_form() {
    let request = prepare_get_index_input(
        &serde_json::json!({
            "vector_bucket_name": "demo-bucket",
            "index_name": "demo-index"
        }),
        &Context::new(),
    )
    .unwrap()
    .build()
    .unwrap();

    assert_eq!(request.vector_bucket_name(), Some("demo-bucket"));
    assert_eq!(request.index_name(), Some("demo-index"));
    assert_eq!(request.index_arn(), None);
}

#[test]
fn get_index_arn_input_sets_only_arn_form() {
    let arn = "arn:aws:s3vectors:us-east-1:123456789012:bucket/demo/index/demo";
    let request =
        prepare_get_index_input(&serde_json::json!({ "index_arn": arn }), &Context::new())
            .unwrap()
            .build()
            .unwrap();

    assert_eq!(request.vector_bucket_name(), None);
    assert_eq!(request.index_name(), None);
    assert_eq!(request.index_arn(), Some(arn));
}
