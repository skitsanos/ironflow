use super::*;

#[test]
fn create_bucket_input_sets_the_resolved_name() {
    let request = prepare_create_bucket_input(
        &serde_json::json!({ "vector_bucket_name": "demo-bucket" }),
        &Context::new(),
    )
    .unwrap()
    .0
    .build()
    .unwrap();

    assert_eq!(request.vector_bucket_name(), Some("demo-bucket"));
}

#[test]
fn get_bucket_name_input_sets_only_name() {
    let request = prepare_get_bucket_input(
        &serde_json::json!({ "vector_bucket_name": "demo-bucket" }),
        &Context::new(),
    )
    .unwrap()
    .build()
    .unwrap();

    assert_eq!(request.vector_bucket_name(), Some("demo-bucket"));
    assert_eq!(request.vector_bucket_arn(), None);
}

#[test]
fn get_bucket_arn_input_sets_only_arn() {
    let request = prepare_get_bucket_input(
        &serde_json::json!({
            "vector_bucket_arn": "arn:aws:s3vectors:us-east-1:123456789012:bucket/demo"
        }),
        &Context::new(),
    )
    .unwrap()
    .build()
    .unwrap();

    assert_eq!(request.vector_bucket_name(), None);
    assert_eq!(
        request.vector_bucket_arn(),
        Some("arn:aws:s3vectors:us-east-1:123456789012:bucket/demo")
    );
}
