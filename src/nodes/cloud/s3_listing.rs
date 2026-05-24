use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::s3_helpers::{build_s3_client, resolve_optional, resolve_output_key, resolve_required};

pub struct S3ListObjectsNode;

#[async_trait]
impl Node for S3ListObjectsNode {
    fn node_type(&self) -> &str {
        "s3_list_objects"
    }

    fn description(&self) -> &str {
        "List objects under a S3 key prefix"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let bucket =
            resolve_required(config, "bucket", Some("S3_BUCKET"), ctx).ok_or_else(|| {
                anyhow::anyhow!("s3_list_objects requires 'bucket' or S3_BUCKET env var")
            })?;
        let prefix = resolve_optional(config, "prefix", None, ctx).unwrap_or_default();
        let delimiter = config
            .get("delimiter")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let output_key = resolve_output_key(config);
        let max_keys = config.get("max_keys").and_then(|value| value.as_u64());
        let max_keys = max_keys.and_then(|value| i32::try_from(value).ok());

        let client = build_s3_client(config, ctx).await?;

        let mut objects = Vec::new();
        let mut continuation_token: Option<String> = None;
        loop {
            let mut request = client
                .list_objects_v2()
                .bucket(bucket.clone())
                .prefix(prefix.clone());
            if !delimiter.is_empty() {
                request = request.delimiter(delimiter);
            }
            if let Some(max_keys) = max_keys {
                request = request.max_keys(max_keys);
            }
            if let Some(token) = continuation_token.clone() {
                request = request.continuation_token(token);
            }

            let response = request.send().await?;
            for item in response.contents() {
                let storage_class = item.storage_class().map(|value| value.as_str().to_string());
                let last_modified = item
                    .last_modified()
                    .map(|value| value.to_string())
                    .unwrap_or_default();

                objects.push(serde_json::json!({
                    "key": item.key().unwrap_or_default(),
                    "size": item.size().unwrap_or_default(),
                    "etag": item.e_tag().unwrap_or_default(),
                    "last_modified": last_modified,
                    "storage_class": storage_class.unwrap_or_default(),
                }));
            }

            if response.is_truncated().unwrap_or(false) {
                continuation_token = response.next_continuation_token().map(str::to_string);
            } else {
                break;
            }
        }

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_bucket", output_key),
            serde_json::Value::String(bucket),
        );
        output.insert(
            format!("{}_prefix", output_key),
            serde_json::Value::String(prefix),
        );
        output.insert(
            format!("{}_count", output_key),
            serde_json::json!(objects.len()),
        );
        output.insert(
            format!("{}_objects", output_key),
            serde_json::Value::Array(objects),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct S3ListBucketsNode;

#[async_trait]
impl Node for S3ListBucketsNode {
    fn node_type(&self) -> &str {
        "s3_list_buckets"
    }

    fn description(&self) -> &str {
        "List available buckets in the S3 account"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let client = build_s3_client(config, ctx).await?;
        let response = client.list_buckets().send().await?;

        let mut buckets = Vec::new();
        for item in response.buckets() {
            let creation_date = item
                .creation_date()
                .map(|value| value.to_string())
                .unwrap_or_default();
            buckets.push(serde_json::json!({
                "name": item.name().unwrap_or_default(),
                "creation_date": creation_date,
            }));
        }

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_count", output_key),
            serde_json::json!(buckets.len()),
        );
        output.insert(
            format!("{}_buckets", output_key),
            serde_json::Value::Array(buckets),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}
