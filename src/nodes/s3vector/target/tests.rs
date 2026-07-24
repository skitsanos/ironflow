use super::*;

mod create;
mod precedence;

fn env<'a>(entries: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |key| {
        entries
            .iter()
            .find_map(|(candidate, value)| (*candidate == key).then(|| (*value).to_string()))
    }
}

#[test]
fn explicit_bucket_arn_does_not_read_environment_name() {
    let config = serde_json::json!({ "vector_bucket_arn": "explicit-arn" });
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
        BucketTarget::Arn("explicit-arn".to_string())
    );
}

#[test]
fn explicit_index_name_is_not_completed_from_environment() {
    let config = serde_json::json!({ "index_name": "explicit-index" });
    let environment = env(&[("S3VECTOR_BUCKET_NAME", "environment-bucket")]);

    assert_eq!(
        resolve_index_target_with_env(
            &config,
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &environment,
        )
        .unwrap_err()
        .to_string(),
        "node requires 'vector_bucket_name' when using 'index_name'"
    );
}

#[test]
fn explicit_index_arn_does_not_read_environment_bucket() {
    let config = serde_json::json!({ "index_arn": "explicit-index-arn" });
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
        IndexTarget::Arn("explicit-index-arn".to_string())
    );
}

#[test]
fn explicit_index_arn_rejects_bucket_fields() {
    let config = serde_json::json!({
        "index_arn": "explicit-index-arn",
        "vector_bucket_name": "extraneous-bucket"
    });

    assert_eq!(
        resolve_index_target_with_env(
            &config,
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &env(&[]),
        )
        .unwrap_err()
        .to_string(),
        "node does not accept bucket identifiers when using 'index_arn'"
    );
}

#[test]
fn explicit_identifier_forms_are_mutually_exclusive() {
    let config = serde_json::json!({
        "vector_bucket_name": "bucket",
        "vector_bucket_arn": "bucket-arn"
    });

    assert_eq!(
        resolve_bucket_target_with_env(
            &config,
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &env(&[]),
        )
        .unwrap_err()
        .to_string(),
        "node requires exactly one of 'vector_bucket_name' or 'vector_bucket_arn'"
    );
}

#[test]
fn conflicting_config_aliases_are_rejected() {
    let config = serde_json::json!({
        "vector_bucket_name": "first",
        "bucket": "second"
    });

    assert_eq!(
        resolve_bucket_target_with_env(
            &config,
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &env(&[]),
        )
        .unwrap_err()
        .to_string(),
        "node requires 'vector_bucket_name' and 'bucket' to resolve to the same value"
    );
}

#[test]
fn invalid_explicit_type_does_not_fall_back_to_environment() {
    let config = serde_json::json!({ "index_arn": 42 });
    let environment = env(&[("S3VECTOR_INDEX_ARN", "environment-index-arn")]);

    assert_eq!(
        resolve_index_target_with_env(
            &config,
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &environment,
        )
        .unwrap_err()
        .to_string(),
        "node requires 'index_arn' to be a string"
    );
}

#[test]
fn interpolated_blank_identifier_is_rejected() {
    let config = serde_json::json!({ "vector_bucket_name": "${ctx.missing}" });

    assert_eq!(
        resolve_bucket_target_with_env(
            &config,
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &env(&[("S3VECTOR_BUCKET_NAME", "environment-bucket")]),
        )
        .unwrap_err()
        .to_string(),
        "node requires 'vector_bucket_name' to be non-empty"
    );
}

#[test]
fn service_specific_bucket_environment_precedes_legacy_s3_bucket() {
    let environment = env(&[
        ("S3VECTOR_BUCKET_NAME", "vector-bucket"),
        ("S3_BUCKET", "object-bucket"),
    ]);

    assert_eq!(
        resolve_bucket_target_with_env(
            &serde_json::json!({}),
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &environment,
        )
        .unwrap(),
        BucketTarget::Name("vector-bucket".to_string())
    );
}

#[test]
fn environment_name_and_arn_forms_are_ambiguous() {
    let environment = env(&[
        ("S3VECTOR_BUCKET_NAME", "bucket"),
        ("S3VECTOR_BUCKET_ARN", "bucket-arn"),
    ]);

    assert_eq!(
        resolve_bucket_target_with_env(
            &serde_json::json!({}),
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &environment,
        )
        .unwrap_err()
        .to_string(),
        "node requires exactly one of 'vector_bucket_name' or 'vector_bucket_arn'"
    );
}

#[test]
fn environment_named_index_requires_complete_name_pair() {
    let complete = env(&[
        ("S3VECTOR_BUCKET_NAME", "bucket"),
        ("S3VECTOR_INDEX_NAME", "index"),
    ]);
    assert_eq!(
        resolve_index_target_with_env(
            &serde_json::json!({}),
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &complete,
        )
        .unwrap(),
        IndexTarget::Names {
            bucket_name: "bucket".to_string(),
            index_name: "index".to_string(),
        }
    );

    let incomplete = env(&[("S3VECTOR_INDEX_NAME", "index")]);
    assert!(
        resolve_index_target_with_env(
            &serde_json::json!({}),
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &incomplete,
        )
        .is_err()
    );
}

#[test]
fn environment_index_arn_rejects_bucket_identifiers() {
    let environment = env(&[
        ("S3VECTOR_INDEX_ARN", "index-arn"),
        ("S3_BUCKET", "legacy-bucket"),
    ]);

    assert_eq!(
        resolve_index_target_with_env(
            &serde_json::json!({}),
            &Context::new(),
            "node",
            TargetPolicy::AllowEnvironment,
            &environment,
        )
        .unwrap_err()
        .to_string(),
        "node does not accept bucket identifiers when using 'index_arn'"
    );
}

#[test]
fn explicit_only_policy_never_reads_environment() {
    let no_env = |_key: &str| -> Option<String> { panic!("environment must not be read") };

    assert_eq!(
        resolve_index_target_with_env(
            &serde_json::json!({}),
            &Context::new(),
            "node",
            TargetPolicy::ExplicitOnly,
            &no_env,
        )
        .unwrap_err()
        .to_string(),
        "node requires 'index_name' or 'index_arn'"
    );
}
