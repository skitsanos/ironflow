use super::*;

fn ctx_with(entries: &[(&str, &str)]) -> Context {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_string(), serde_json::json!(value)))
        .collect()
}

#[test]
fn delete_index_target_interpolates_bucket_and_index_names() {
    let ctx = ctx_with(&[("bucket", "workflow-bucket"), ("index", "workflow-index")]);
    let config = serde_json::json!({
        "vector_bucket_name": "${ctx.bucket}",
        "index_name": "${ctx.index}"
    });

    assert_eq!(
        resolve_index_target(&config, &ctx).unwrap(),
        IndexTarget::Name {
            bucket_name: "workflow-bucket".to_string(),
            index_name: "workflow-index".to_string(),
        }
    );
}

#[test]
fn delete_index_target_interpolates_index_arn() {
    let ctx = ctx_with(&[(
        "index_arn",
        "arn:aws:s3vectors:us-east-1:123456789012:bucket/demo/index/demo",
    )]);
    let config = serde_json::json!({ "index_arn": "${ctx.index_arn}" });

    assert_eq!(
        resolve_index_target(&config, &ctx).unwrap(),
        IndexTarget::Arn(
            "arn:aws:s3vectors:us-east-1:123456789012:bucket/demo/index/demo".to_string()
        )
    );
}

#[test]
fn delete_bucket_target_interpolates_bucket_arn() {
    let ctx = ctx_with(&[(
        "bucket_arn",
        "arn:aws:s3vectors:us-east-1:123456789012:bucket/demo",
    )]);
    let config = serde_json::json!({ "vector_bucket_arn": "${ctx.bucket_arn}" });

    assert_eq!(
        resolve_bucket_target(&config, &ctx).unwrap(),
        BucketTarget::Arn("arn:aws:s3vectors:us-east-1:123456789012:bucket/demo".to_string())
    );
}

#[test]
fn delete_index_target_rejects_ambiguous_identifiers() {
    let config = serde_json::json!({
        "index_name": "explicit-name",
        "index_arn": "arn:aws:s3vectors:us-east-1:123456789012:bucket/demo/index/explicit"
    });

    assert_eq!(
        resolve_index_target(&config, &Context::new())
            .unwrap_err()
            .to_string(),
        "s3vector_delete_index requires exactly one of 'index_name' or 'index_arn'"
    );
}

#[test]
fn delete_bucket_target_rejects_ambiguous_identifiers() {
    let config = serde_json::json!({
        "vector_bucket_name": "explicit-name",
        "vector_bucket_arn": "arn:aws:s3vectors:us-east-1:123456789012:bucket/explicit"
    });

    assert_eq!(
        resolve_bucket_target(&config, &Context::new())
            .unwrap_err()
            .to_string(),
        "s3vector_delete_bucket requires exactly one of 'vector_bucket_name' or 'vector_bucket_arn'"
    );
}

#[test]
fn delete_index_name_rejects_ambiguous_bucket_identifiers() {
    let config = serde_json::json!({
        "index_name": "explicit-index",
        "vector_bucket_name": "explicit-bucket",
        "vector_bucket_arn": "arn:aws:s3vectors:us-east-1:123456789012:bucket/explicit"
    });

    assert_eq!(
        resolve_index_target(&config, &Context::new())
            .unwrap_err()
            .to_string(),
        "s3vector_delete_index requires exactly one of 'vector_bucket_name' or 'vector_bucket_arn'"
    );
}

#[test]
fn delete_targets_do_not_fall_back_to_environment_identifiers() {
    let config = serde_json::json!({});

    assert_eq!(
        resolve_bucket_target(&config, &Context::new())
            .unwrap_err()
            .to_string(),
        "s3vector_delete_bucket requires 'vector_bucket_name' or 'vector_bucket_arn'"
    );
    assert_eq!(
        resolve_index_target(&config, &Context::new())
            .unwrap_err()
            .to_string(),
        "s3vector_delete_index requires 'index_name' or 'index_arn'"
    );
}
