use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::{resolve_bucket_id, resolve_output_key};
use super::parameters::resolve_non_empty_string;

pub struct S3VectorCreateBucketNode;

#[async_trait]
impl Node for S3VectorCreateBucketNode {
    fn node_type(&self) -> &str {
        "s3vector_create_bucket"
    }

    fn description(&self) -> &str {
        "Create an S3 Vector bucket"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let bucket_name = resolve_non_empty_string(
            config,
            &["vector_bucket_name", "bucket"],
            &["S3VECTOR_BUCKET_NAME", "S3_BUCKET"],
            ctx,
            "s3vector_create_bucket",
            "vector_bucket_name",
        )?;
        let output_key = resolve_output_key(config);

        let client = build_s3vector_client(config, ctx).await?;
        let response = client
            .create_vector_bucket()
            .vector_bucket_name(bucket_name.clone())
            .send()
            .await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_bucket_name", output_key),
            serde_json::Value::String(bucket_name),
        );
        if let Some(arn) = response.vector_bucket_arn() {
            output.insert(
                format!("{}_bucket_arn", output_key),
                serde_json::Value::String(arn.to_string()),
            );
        }
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct S3VectorGetBucketNode;

#[async_trait]
impl Node for S3VectorGetBucketNode {
    fn node_type(&self) -> &str {
        "s3vector_get_bucket"
    }

    fn description(&self) -> &str {
        "Get metadata for an S3 Vector bucket"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (bucket_name, bucket_arn) = resolve_bucket_id(config, ctx, "s3vector_get_bucket")?;
        if bucket_name.is_none() && bucket_arn.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_get_bucket requires 'vector_bucket_name' or 'vector_bucket_arn'"
            ));
        }

        let mut request = build_s3vector_client(config, ctx)
            .await?
            .get_vector_bucket();

        if let Some(name) = bucket_name {
            request = request.vector_bucket_name(name);
        }
        if let Some(arn) = bucket_arn {
            request = request.vector_bucket_arn(arn);
        }

        let response = request.send().await?;
        let bucket = response.vector_bucket();

        let mut output = NodeOutput::new();
        if let Some(bucket) = bucket {
            output.insert(
                format!("{}_bucket_name", output_key),
                serde_json::Value::String(bucket.vector_bucket_name().to_string()),
            );
            output.insert(
                format!("{}_bucket_arn", output_key),
                serde_json::Value::String(bucket.vector_bucket_arn().to_string()),
            );
            output.insert(
                format!("{}_creation_time", output_key),
                serde_json::Value::String(bucket.creation_time().to_string()),
            );
            if let Some(encryption_configuration) = bucket.encryption_configuration() {
                output.insert(
                    format!("{}_encryption_configuration", output_key),
                    serde_json::json!(format!("{:?}", encryption_configuration)),
                );
            }
        }

        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}
