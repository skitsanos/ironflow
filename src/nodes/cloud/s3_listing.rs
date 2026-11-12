use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::s3_helpers::{build_s3_client, resolve_optional, resolve_output_key, resolve_required};
use super::s3_list_pages::{ListBounds, Next, Pager};

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
        let start_token = resolve_optional(config, "continuation_token", None, ctx)
            .filter(|token| !token.is_empty());
        let mut pager = Pager::new(
            ListBounds::from_config(config, ctx)?,
            start_token.as_deref(),
        );

        let client = build_s3_client(config, ctx).await?;

        let mut objects = Vec::new();
        let mut common_prefixes = Vec::new();
        let mut continuation_token = start_token;
        let next_token = loop {
            let mut request = client
                .list_objects_v2()
                .bucket(bucket.clone())
                .prefix(prefix.clone())
                .max_keys(pager.page_keys());
            if !delimiter.is_empty() {
                request = request.delimiter(delimiter);
            }
            if let Some(token) = continuation_token.take() {
                request = request.continuation_token(token);
            }

            let response = request.send().await?;
            pager.admit_page(response.contents().len() + response.common_prefixes().len())?;
            objects.extend(response.contents().iter().map(object_entry));
            common_prefixes.extend(
                response
                    .common_prefixes()
                    .iter()
                    .map(|item| serde_json::json!(item.prefix().unwrap_or_default())),
            );

            let truncated = response.is_truncated().unwrap_or(false);
            match pager.next(truncated, response.next_continuation_token())? {
                Next::Page(token) => continuation_token = Some(token),
                Next::Done(token) => break token,
            }
        };

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
            format!("{}_common_prefixes", output_key),
            serde_json::Value::Array(common_prefixes),
        );
        output.insert(
            format!("{}_truncated", output_key),
            serde_json::Value::Bool(next_token.is_some()),
        );
        output.insert(
            format!("{}_next_continuation_token", output_key),
            serde_json::Value::String(next_token.unwrap_or_default()),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

fn object_entry(item: &aws_sdk_s3::types::Object) -> serde_json::Value {
    let last_modified = item
        .last_modified()
        .map(|value| value.to_string())
        .unwrap_or_default();
    serde_json::json!({
        "key": item.key().unwrap_or_default(),
        "size": item.size().unwrap_or_default(),
        "etag": item.e_tag().unwrap_or_default(),
        "last_modified": last_modified,
        "storage_class": item
            .storage_class()
            .map(|value| value.as_str())
            .unwrap_or_default(),
    })
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
