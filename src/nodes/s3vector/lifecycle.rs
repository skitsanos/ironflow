use anyhow::{Context as _, Result};
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::{resolve_optional, resolve_output_key};

#[derive(Debug, PartialEq)]
enum ResourceIdentifier {
    Name(String),
    Arn(String),
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn resolve_identifier(
    config: &serde_json::Value,
    ctx: &Context,
    name_keys: &[&str],
    arn_keys: &[&str],
    node: &str,
    name_field: &str,
    arn_field: &str,
) -> Result<Option<ResourceIdentifier>> {
    let name = non_empty(resolve_optional(config, name_keys, &[], ctx));
    let arn = non_empty(resolve_optional(config, arn_keys, &[], ctx));

    match (name, arn) {
        (Some(_), Some(_)) => anyhow::bail!(
            "{} requires exactly one of '{}' or '{}'",
            node,
            name_field,
            arn_field
        ),
        (Some(name), None) => Ok(Some(ResourceIdentifier::Name(name))),
        (None, Some(arn)) => Ok(Some(ResourceIdentifier::Arn(arn))),
        (None, None) => Ok(None),
    }
}

fn resolve_bucket_identifier(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
) -> Result<Option<ResourceIdentifier>> {
    resolve_identifier(
        config,
        ctx,
        &["vector_bucket_name", "bucket"],
        &["vector_bucket_arn"],
        node,
        "vector_bucket_name",
        "vector_bucket_arn",
    )
}

fn resolve_index_identifier(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<Option<ResourceIdentifier>> {
    resolve_identifier(
        config,
        ctx,
        &["index_name", "index"],
        &["index_arn"],
        "s3vector_delete_index",
        "index_name",
        "index_arn",
    )
}

#[derive(Debug, PartialEq)]
enum BucketTarget {
    Name(String),
    Arn(String),
}

fn resolve_bucket_target(config: &serde_json::Value, ctx: &Context) -> Result<BucketTarget> {
    match resolve_bucket_identifier(config, ctx, "s3vector_delete_bucket")? {
        Some(ResourceIdentifier::Name(name)) => return Ok(BucketTarget::Name(name)),
        Some(ResourceIdentifier::Arn(arn)) => return Ok(BucketTarget::Arn(arn)),
        None => {}
    }

    anyhow::bail!("s3vector_delete_bucket requires 'vector_bucket_name' or 'vector_bucket_arn'")
}

#[derive(Debug, PartialEq)]
enum IndexTarget {
    Name {
        bucket_name: String,
        index_name: String,
    },
    Arn(String),
}

fn resolve_index_target(config: &serde_json::Value, ctx: &Context) -> Result<IndexTarget> {
    match resolve_index_identifier(config, ctx)? {
        Some(ResourceIdentifier::Name(index_name)) => {
            let bucket_name = match resolve_bucket_identifier(config, ctx, "s3vector_delete_index")?
            {
                Some(ResourceIdentifier::Name(bucket_name)) => bucket_name,
                Some(ResourceIdentifier::Arn(_)) | None => {
                    return Err(anyhow::anyhow!(
                        "s3vector_delete_index requires 'vector_bucket_name' when using 'index_name'"
                    ));
                }
            };
            Ok(IndexTarget::Name {
                bucket_name,
                index_name,
            })
        }
        Some(ResourceIdentifier::Arn(index_arn)) => Ok(IndexTarget::Arn(index_arn)),
        None => Err(anyhow::anyhow!(
            "s3vector_delete_index requires 'index_name' or 'index_arn'"
        )),
    }
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
        let target = resolve_index_target(config, ctx)?;
        let mut request = build_s3vector_client(config, ctx).await?.delete_index();

        let mut output = NodeOutput::new();
        match target {
            IndexTarget::Name {
                bucket_name,
                index_name,
            } => {
                request = request
                    .vector_bucket_name(bucket_name.clone())
                    .index_name(index_name.clone());
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
                request = request.index_arn(index_arn.clone());
                output.insert(
                    format!("{}_index_arn", output_key),
                    serde_json::Value::String(index_arn),
                );
            }
        }

        request
            .send()
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
        let target = resolve_bucket_target(config, ctx)?;
        let mut request = build_s3vector_client(config, ctx)
            .await?
            .delete_vector_bucket();

        let mut output = NodeOutput::new();
        match target {
            BucketTarget::Name(bucket_name) => {
                request = request.vector_bucket_name(bucket_name.clone());
                output.insert(
                    format!("{}_bucket_name", output_key),
                    serde_json::Value::String(bucket_name),
                );
            }
            BucketTarget::Arn(bucket_arn) => {
                request = request.vector_bucket_arn(bucket_arn.clone());
                output.insert(
                    format!("{}_bucket_arn", output_key),
                    serde_json::Value::String(bucket_arn),
                );
            }
        }

        request
            .send()
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
