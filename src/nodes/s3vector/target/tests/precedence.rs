use super::super::*;
use super::env;

#[test]
fn explicit_bucket_name_does_not_read_environment_arn() {
    let config = serde_json::json!({ "vector_bucket_name": "explicit-bucket" });
    let no_env = |_key: &str| -> Option<String> { panic!("environment must not be read") };

    assert_eq!(
        resolve_bucket_target_with_env(
            &config,
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &no_env,
        )
        .unwrap(),
        BucketTarget::Name("explicit-bucket".to_string())
    );
}

#[test]
fn explicit_index_names_do_not_read_environment_arn() {
    let config = serde_json::json!({
        "vector_bucket_name": "explicit-bucket",
        "index_name": "explicit-index"
    });
    let no_env = |_key: &str| -> Option<String> { panic!("environment must not be read") };

    assert_eq!(
        resolve_index_target_with_env(
            &config,
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &no_env,
        )
        .unwrap(),
        IndexTarget::Names {
            bucket_name: "explicit-bucket".to_string(),
            index_name: "explicit-index".to_string(),
        }
    );
}

#[test]
fn environment_only_bucket_arn_is_supported() {
    let environment = env(&[("S3VECTOR_BUCKET_ARN", "environment-bucket-arn")]);

    assert_eq!(
        resolve_bucket_target_with_env(
            &serde_json::json!({}),
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &environment,
        )
        .unwrap(),
        BucketTarget::Arn("environment-bucket-arn".to_string())
    );
}

#[test]
fn environment_only_index_arn_is_supported() {
    let environment = env(&[("S3VECTOR_INDEX_ARN", "environment-index-arn")]);

    assert_eq!(
        resolve_index_target_with_env(
            &serde_json::json!({}),
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &environment,
        )
        .unwrap(),
        IndexTarget::Arn("environment-index-arn".to_string())
    );
}
