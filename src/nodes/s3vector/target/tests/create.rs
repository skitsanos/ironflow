use super::super::*;

fn empty_env(_key: &str) -> Option<String> {
    None
}

#[test]
fn create_index_supports_bucket_arn_with_index_name() {
    let config = serde_json::json!({
        "vector_bucket_arn": "bucket-arn",
        "index_name": "index"
    });

    assert_eq!(
        resolve_create_index_target_with_env(&config, &Context::new(), "node", &empty_env).unwrap(),
        CreateIndexTarget {
            bucket: BucketTarget::Arn("bucket-arn".to_string()),
            index_name: "index".to_string(),
        }
    );
}

#[test]
fn create_bucket_rejects_existing_bucket_arn() {
    let config = serde_json::json!({ "vector_bucket_arn": "bucket-arn" });

    assert_eq!(
        resolve_create_bucket_name_with_env(&config, &Context::new(), "node", &empty_env)
            .unwrap_err()
            .to_string(),
        "node does not support 'vector_bucket_arn'; use 'vector_bucket_name'"
    );
}

#[test]
fn matching_canonical_and_alias_values_are_accepted() {
    let config = serde_json::json!({
        "vector_bucket_name": "same-bucket",
        "bucket": "same-bucket"
    });

    assert_eq!(
        resolve_bucket_target_with_env(
            &config,
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &empty_env,
        )
        .unwrap(),
        BucketTarget::Name("same-bucket".to_string())
    );
}
