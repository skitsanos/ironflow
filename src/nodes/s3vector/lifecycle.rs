use anyhow::{Context as _, Result};
use async_trait::async_trait;
use aws_sdk_s3vectors::operation::delete_index::builders::DeleteIndexInputBuilder;
use aws_sdk_s3vectors::operation::delete_vector_bucket::builders::DeleteVectorBucketInputBuilder;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::resolve_output_key;
use super::target::{
    BucketTarget, IndexTarget, TargetPolicy, resolve_bucket_target, resolve_index_target,
};

fn prepare_delete_index_input(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<(DeleteIndexInputBuilder, IndexTarget)> {
    let target = resolve_index_target(
        config,
        ctx,
        "s3vector_delete_index",
        TargetPolicy::ExplicitOnly,
    )?;
    let request = match &target {
        IndexTarget::Names {
            bucket_name,
            index_name,
        } => DeleteIndexInputBuilder::default()
            .vector_bucket_name(bucket_name.clone())
            .index_name(index_name.clone()),
        IndexTarget::Arn(index_arn) => {
            DeleteIndexInputBuilder::default().index_arn(index_arn.clone())
        }
    };
    Ok((request, target))
}

fn prepare_delete_bucket_input(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<(DeleteVectorBucketInputBuilder, BucketTarget)> {
    let target = resolve_bucket_target(
        config,
        ctx,
        "s3vector_delete_bucket",
        TargetPolicy::ExplicitOnly,
    )?;
    let request = match &target {
        BucketTarget::Name(bucket_name) => {
            DeleteVectorBucketInputBuilder::default().vector_bucket_name(bucket_name.clone())
        }
        BucketTarget::Arn(bucket_arn) => {
            DeleteVectorBucketInputBuilder::default().vector_bucket_arn(bucket_arn.clone())
        }
    };
    Ok((request, target))
}

pub struct S3VectorDeleteIndexNode;

#[async_trait]
impl Node for S3VectorDeleteIndexNode {
    fn node_type(&self) -> &str {
        "s3vector_delete_index"
    }

    fn description(&self) -> &str {
        "Delete an S3 Vector index"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (request, target) = prepare_delete_index_input(config, ctx)?;
        let client = build_s3vector_client(config, ctx).await?;

        let mut output = NodeOutput::new();
        match target {
            IndexTarget::Names {
                bucket_name,
                index_name,
            } => {
                output.insert(
                    format!("{}_bucket_name", output_key),
                    serde_json::Value::String(bucket_name),
                );
                output.insert(
                    format!("{}_index_name", output_key),
                    serde_json::Value::String(index_name),
                );
            }
            IndexTarget::Arn(index_arn) => {
                output.insert(
                    format!("{}_index_arn", output_key),
                    serde_json::Value::String(index_arn),
                );
            }
        }

        request
            .send_with(&client)
            .await
            .context("s3vector_delete_index request failed")?;
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct S3VectorDeleteBucketNode;

#[async_trait]
impl Node for S3VectorDeleteBucketNode {
    fn node_type(&self) -> &str {
        "s3vector_delete_bucket"
    }

    fn description(&self) -> &str {
        "Delete an S3 Vector bucket"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (request, target) = prepare_delete_bucket_input(config, ctx)?;
        let client = build_s3vector_client(config, ctx).await?;

        let mut output = NodeOutput::new();
        match target {
            BucketTarget::Name(bucket_name) => {
                output.insert(
                    format!("{}_bucket_name", output_key),
                    serde_json::Value::String(bucket_name),
                );
            }
            BucketTarget::Arn(bucket_arn) => {
                output.insert(
                    format!("{}_bucket_arn", output_key),
                    serde_json::Value::String(bucket_arn),
                );
            }
        }

        request
            .send_with(&client)
            .await
            .context("s3vector_delete_bucket request failed")?;
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

#[cfg(test)]
mod tests;
