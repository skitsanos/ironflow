use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::s3_helpers::{
    build_s3_client, resolve_content_length, resolve_expires_in, resolve_optional,
    resolve_output_key, resolve_required,
};

pub struct S3PresignUrlNode;

#[async_trait]
impl Node for S3PresignUrlNode {
    fn node_type(&self) -> &str {
        "s3_presign_url"
    }

    fn description(&self) -> &str {
        "Generate a presigned URL for a supported S3 operation"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let bucket =
            resolve_required(config, "bucket", Some("S3_BUCKET"), ctx).ok_or_else(|| {
                anyhow::anyhow!("s3_presign_url requires 'bucket' or S3_BUCKET env var")
            })?;
        let key = resolve_required(config, "key", None, ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_presign_url requires 'key'"))?;
        let method = config
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or("GET")
            .to_ascii_uppercase();
        let expires_in = resolve_expires_in(config)?;
        let output_key = resolve_output_key(config);
        let content_type = resolve_optional(config, "content_type", None, ctx)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let content_length = resolve_content_length(config);

        let client = build_s3_client(config, ctx).await?;
        let presign_config =
            aws_sdk_s3::presigning::PresigningConfig::expires_in(Duration::from_secs(expires_in))
                .map_err(|error| anyhow::anyhow!("s3_presign_url invalid expires_in: {}", error))?;

        let request = match method.as_str() {
            "GET" => {
                client
                    .get_object()
                    .bucket(bucket.clone())
                    .key(key.clone())
                    .presigned(presign_config)
                    .await?
            }
            "PUT" => {
                let mut request = client
                    .put_object()
                    .bucket(bucket.clone())
                    .key(key.clone())
                    .content_type(content_type);
                if let Some(content_length) = content_length {
                    request = request.content_length(content_length);
                }
                request.presigned(presign_config).await?
            }
            "HEAD" => {
                client
                    .head_object()
                    .bucket(bucket.clone())
                    .key(key.clone())
                    .presigned(presign_config)
                    .await?
            }
            "DELETE" => {
                client
                    .delete_object()
                    .bucket(bucket.clone())
                    .key(key.clone())
                    .presigned(presign_config)
                    .await?
            }
            _ => {
                anyhow::bail!(
                    "s3_presign_url method '{}' is not supported. Use GET, PUT, HEAD, or DELETE",
                    method
                );
            }
        };

        let mut headers = serde_json::Map::new();
        for (name, value) in request.headers() {
            headers.insert(
                name.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }

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
            format!("{}_method", output_key),
            serde_json::Value::String(method),
        );
        output.insert(
            format!("{}_expires_in", output_key),
            serde_json::json!(expires_in),
        );
        output.insert(
            format!("{}_url", output_key),
            serde_json::Value::String(request.uri().to_string()),
        );
        output.insert(
            format!("{}_headers", output_key),
            serde_json::Value::Object(headers),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}
