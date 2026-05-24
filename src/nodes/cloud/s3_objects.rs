use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::s3_helpers::{
    build_s3_client, resolve_optional, resolve_output_key, resolve_payload_bytes, resolve_required,
    write_payload_to_output,
};

pub struct S3PutObjectNode;

#[async_trait]
impl Node for S3PutObjectNode {
    fn node_type(&self) -> &str {
        "s3_put_object"
    }

    fn description(&self) -> &str {
        "Upload an object to S3 (or S3-compatible storage) from text or base64 input"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let bucket =
            resolve_required(config, "bucket", Some("S3_BUCKET"), ctx).ok_or_else(|| {
                anyhow::anyhow!("s3_put_object requires 'bucket' or S3_BUCKET env var")
            })?;
        let key = resolve_required(config, "key", None, ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_put_object requires 'key'"))?;
        let output_key = resolve_output_key(config);
        let content_type = resolve_optional(config, "content_type", None, ctx)
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let body = resolve_payload_bytes(config, ctx).await?;
        let client = build_s3_client(config, ctx).await?;
        let response = client
            .put_object()
            .bucket(bucket.clone())
            .key(key.clone())
            .content_type(content_type.clone())
            .body(ByteStream::from(body))
            .send()
            .await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_bucket", output_key),
            serde_json::Value::String(bucket),
        );
        output.insert(
            format!("{}_key", output_key),
            serde_json::Value::String(key),
        );
        output.insert(
            format!("{}_content_type", output_key),
            serde_json::Value::String(content_type),
        );
        if let Some(etag) = response.e_tag() {
            output.insert(
                format!("{}_etag", output_key),
                serde_json::Value::String(etag.to_string()),
            );
        }
        if let Some(version_id) = response.version_id() {
            output.insert(
                format!("{}_version_id", output_key),
                serde_json::Value::String(version_id.to_string()),
            );
        }
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct S3GetObjectNode;

#[async_trait]
impl Node for S3GetObjectNode {
    fn node_type(&self) -> &str {
        "s3_get_object"
    }

    fn description(&self) -> &str {
        "Download an object from S3"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let bucket =
            resolve_required(config, "bucket", Some("S3_BUCKET"), ctx).ok_or_else(|| {
                anyhow::anyhow!("s3_get_object requires 'bucket' or S3_BUCKET env var")
            })?;
        let key = resolve_required(config, "key", None, ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_get_object requires 'key'"))?;
        let output_key = resolve_output_key(config);
        let output_encoding = config
            .get("encoding")
            .and_then(|value| value.as_str())
            .unwrap_or("text");

        let client = build_s3_client(config, ctx).await?;
        let response = client
            .get_object()
            .bucket(bucket.clone())
            .key(key.clone())
            .send()
            .await?;

        let content_type = response.content_type().map(ToString::to_string);
        let content_length = response.content_length();
        let e_tag = response.e_tag().map(ToString::to_string);
        let last_modified = response.last_modified().map(ToString::to_string);

        let bytes = response.body.collect().await?.into_bytes().to_vec();

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_bucket", output_key),
            serde_json::Value::String(bucket),
        );
        output.insert(
            format!("{}_key", output_key),
            serde_json::Value::String(key),
        );
        if let Some(content_type) = content_type {
            output.insert(
                format!("{}_content_type", output_key),
                serde_json::Value::String(content_type.to_string()),
            );
        }
        if let Some(content_length) = content_length {
            output.insert(
                format!("{}_content_length", output_key),
                serde_json::json!(content_length),
            );
        }
        if let Some(etag) = e_tag {
            output.insert(
                format!("{}_etag", output_key),
                serde_json::Value::String(etag.to_string()),
            );
        }
        if let Some(last_modified) = last_modified {
            output.insert(
                format!("{}_last_modified", output_key),
                serde_json::Value::String(last_modified.to_string()),
            );
        }

        write_payload_to_output(&mut output, &output_key, &bytes, output_encoding)?;
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct S3DeleteObjectNode;

#[async_trait]
impl Node for S3DeleteObjectNode {
    fn node_type(&self) -> &str {
        "s3_delete_object"
    }

    fn description(&self) -> &str {
        "Delete an object from S3"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let bucket =
            resolve_required(config, "bucket", Some("S3_BUCKET"), ctx).ok_or_else(|| {
                anyhow::anyhow!("s3_delete_object requires 'bucket' or S3_BUCKET env var")
            })?;
        let key = resolve_required(config, "key", None, ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_delete_object requires 'key'"))?;
        let output_key = resolve_output_key(config);
        let version_id = resolve_optional(config, "version_id", None, ctx);

        let client = build_s3_client(config, ctx).await?;
        let mut request = client
            .delete_object()
            .bucket(bucket.clone())
            .key(key.clone());
        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }
        let response = request.send().await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_bucket", output_key),
            serde_json::Value::String(bucket),
        );
        output.insert(
            format!("{}_key", output_key),
            serde_json::Value::String(key),
        );
        if let Some(delete_marker) = response.delete_marker() {
            output.insert(
                format!("{}_delete_marker", output_key),
                serde_json::Value::Bool(delete_marker),
            );
        }
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct S3CopyObjectNode;

#[async_trait]
impl Node for S3CopyObjectNode {
    fn node_type(&self) -> &str {
        "s3_copy_object"
    }

    fn description(&self) -> &str {
        "Copy an S3 object to another key or bucket"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_bucket = resolve_required(config, "source_bucket", Some("S3_BUCKET"), ctx)
            .ok_or_else(|| {
                anyhow::anyhow!("s3_copy_object requires 'source_bucket' or S3_BUCKET env var")
            })?;
        let source_key = resolve_required(config, "source_key", None, ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_copy_object requires 'source_key'"))?;
        let destination_bucket = resolve_required(config, "bucket", Some("S3_BUCKET"), ctx)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "s3_copy_object requires 'bucket' (destination bucket) or S3_BUCKET env var"
                )
            })?;
        let destination_key = resolve_required(config, "key", None, ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_copy_object requires destination 'key'"))?;
        let output_key = resolve_output_key(config);
        let copy_source = format!("{}/{}", source_bucket, source_key);

        let client = build_s3_client(config, ctx).await?;
        let response = client
            .copy_object()
            .bucket(destination_bucket.clone())
            .key(destination_key.clone())
            .copy_source(copy_source)
            .send()
            .await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_source_bucket", output_key),
            serde_json::Value::String(source_bucket),
        );
        output.insert(
            format!("{}_source_key", output_key),
            serde_json::Value::String(source_key),
        );
        output.insert(
            format!("{}_destination_bucket", output_key),
            serde_json::Value::String(destination_bucket),
        );
        output.insert(
            format!("{}_destination_key", output_key),
            serde_json::Value::String(destination_key),
        );
        if let Some(copy_result) = response.copy_object_result() {
            if let Some(etag) = copy_result.e_tag() {
                output.insert(
                    format!("{}_etag", output_key),
                    serde_json::Value::String(etag.to_string()),
                );
            }
            if let Some(last_modified) = copy_result.last_modified() {
                output.insert(
                    format!("{}_last_modified", output_key),
                    serde_json::Value::String(last_modified.to_string()),
                );
            }
        }
        if let Some(version_id) = response.version_id() {
            output.insert(
                format!("{}_version_id", output_key),
                serde_json::Value::String(version_id.to_string()),
            );
        }
        if let Some(source_version_id) = response.copy_source_version_id() {
            output.insert(
                format!("{}_source_version_id", output_key),
                serde_json::Value::String(source_version_id.to_string()),
            );
        }
        if let Some(expiration) = response.expiration() {
            output.insert(
                format!("{}_expiration", output_key),
                serde_json::Value::String(expiration.to_string()),
            );
        }
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}
