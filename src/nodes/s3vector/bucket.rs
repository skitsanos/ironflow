use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3vectors::operation::create_vector_bucket::builders::CreateVectorBucketInputBuilder;
use aws_sdk_s3vectors::operation::get_vector_bucket::builders::GetVectorBucketInputBuilder;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::resolve_output_key;
use super::target::{
    BucketTarget, TargetPolicy, resolve_bucket_target, resolve_create_bucket_name,
};

fn prepare_create_bucket_input(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<(CreateVectorBucketInputBuilder, String)> {
    let bucket_name = resolve_create_bucket_name(config, ctx, "s3vector_create_bucket")?;
    let request = CreateVectorBucketInputBuilder::default().vector_bucket_name(bucket_name.clone());
    Ok((request, bucket_name))
}

fn prepare_get_bucket_input(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<GetVectorBucketInputBuilder> {
    let target = resolve_bucket_target(
        config,
        ctx,
        "s3vector_get_bucket",
        TargetPolicy::AllowEnvironment,
    )?;
    let request = match target {
        BucketTarget::Name(name) => GetVectorBucketInputBuilder::default().vector_bucket_name(name),
        BucketTarget::Arn(arn) => GetVectorBucketInputBuilder::default().vector_bucket_arn(arn),
    };
    Ok(request)
}

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
        let (request, bucket_name) = prepare_create_bucket_input(config, ctx)?;
        let output_key = resolve_output_key(config);

        let client = build_s3vector_client(config, ctx).await?;
        let response = request.send_with(&client).await?;

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
        let request = prepare_get_bucket_input(config, ctx)?;
        let client = build_s3vector_client(config, ctx).await?;
        let response = request.send_with(&client).await?;
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

#[cfg(test)]
mod tests;
