use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::{Client, primitives::ByteStream};
use base64::Engine;
use std::time::Duration;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

fn resolve_required(
    config: &serde_json::Value,
    key: &str,
    env_key: Option<&str>,
    ctx: &Context,
) -> Option<String> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| interpolate_ctx(value, ctx))
        .or_else(|| env_key.and_then(|name| std::env::var(name).ok()))
}

fn resolve_optional(
    config: &serde_json::Value,
    key: &str,
    env_key: Option<&str>,
    ctx: &Context,
) -> Option<String> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| interpolate_ctx(value, ctx))
        .or_else(|| env_key.and_then(|name| std::env::var(name).ok()))
}

fn resolve_output_key(config: &serde_json::Value) -> String {
    config
        .get("output_key")
        .and_then(|value| value.as_str())
        .unwrap_or("s3")
        .to_string()
}

fn resolve_bool(config: &serde_json::Value, key: &str, env_key: Option<&str>) -> bool {
    config
        .get(key)
        .and_then(|value| value.as_bool())
        .or_else(|| env_key.and_then(parse_bool_env))
        .unwrap_or(false)
}

fn parse_bool_env(name: &str) -> Option<bool> {
    std::env::var(name)
        .ok()
        .and_then(|value| match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
}

fn resolve_region(config: &serde_json::Value, ctx: &Context) -> Option<String> {
    resolve_optional(config, "region", Some("S3_REGION"), ctx)
        .or_else(|| std::env::var("AWS_REGION").ok())
        .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
}

fn resolve_expires_in(config: &serde_json::Value) -> Result<u64> {
    let expires_in = config.get("expires_in").and_then(|value| value.as_u64());
    let expires_in = expires_in.unwrap_or(3600);
    if expires_in == 0 || expires_in > 604800 {
        anyhow::bail!("s3_presign_url requires 'expires_in' to be between 1 and 604800 seconds");
    }
    Ok(expires_in)
}

fn resolve_content_length(config: &serde_json::Value) -> Option<i64> {
    let value = config
        .get("content_length")
        .or_else(|| config.get("contentLength"))
        .and_then(|value| value.as_i64())?;
    if value <= 0 {
        return None;
    }
    Some(value)
}

async fn build_s3_client(config: &serde_json::Value, ctx: &Context) -> Result<Client> {
    let force_path_style =
        resolve_bool(config, "force_path_style", Some("AWS_S3_FORCE_PATH_STYLE"));
    let endpoint_url = resolve_optional(config, "endpoint_url", Some("AWS_ENDPOINT_URL"), ctx);
    let region = resolve_region(config, ctx);

    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(region) = region {
        loader = loader.region(Region::new(region));
    }

    let base_config = loader.load().await;
    let mut s3_builder = aws_sdk_s3::config::Builder::from(&base_config);
    s3_builder = s3_builder.force_path_style(force_path_style);
    if let Some(endpoint) = endpoint_url {
        s3_builder = s3_builder.endpoint_url(endpoint);
    }
    Ok(Client::from_conf(s3_builder.build()))
}

async fn resolve_payload_bytes(config: &serde_json::Value, ctx: &Context) -> Result<Vec<u8>> {
    let encoding = config
        .get("encoding")
        .or_else(|| config.get("source_encoding"))
        .and_then(|value| value.as_str())
        .unwrap_or("text");

    if let Some(source_path) = config.get("source_path").and_then(|value| value.as_str()) {
        let source_path = interpolate_ctx(source_path, ctx);
        return Ok(tokio::fs::read(source_path).await?);
    }

    if let Some(source_key) = config.get("source_key").and_then(|value| value.as_str()) {
        let source_key = interpolate_ctx(source_key, ctx);
        let raw = ctx
            .get(&source_key)
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!("source_key '{}' must be a string in context", source_key)
            })?;
        return match encoding {
            "base64" => base64::engine::general_purpose::STANDARD
                .decode(raw)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "Failed to decode base64 source_key '{}': {}",
                        source_key,
                        error
                    )
                }),
            "text" => Ok(raw.as_bytes().to_vec()),
            other => anyhow::bail!(
                "s3 payload encoding '{}' is not supported. Use 'text' or 'base64'",
                other
            ),
        };
    }

    if let Some(content) = config.get("content").and_then(|value| value.as_str()) {
        let content = interpolate_ctx(content, ctx);
        return match encoding {
            "text" => Ok(content.as_bytes().to_vec()),
            "base64" => base64::engine::general_purpose::STANDARD
                .decode(content)
                .map_err(|error| anyhow::anyhow!("Failed to decode base64 content: {}", error)),
            other => anyhow::bail!(
                "s3 payload encoding '{}' is not supported. Use 'text' or 'base64'",
                other
            ),
        };
    }

    anyhow::bail!("S3 node requires one of 'content', 'source_key', or 'source_path'");
}

fn write_payload_to_output(
    output: &mut NodeOutput,
    output_key: &str,
    bytes: &[u8],
    encoding: &str,
) -> Result<()> {
    let body_value = match encoding {
        "base64" => {
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        "text" => {
            let value = String::from_utf8(bytes.to_vec()).map_err(|error| {
                anyhow::anyhow!("Failed to convert response body to UTF-8: {}", error)
            })?;
            serde_json::Value::String(value)
        }
        other => {
            return Err(anyhow::anyhow!(
                "s3 payload encoding '{}' is not supported. Use 'text' or 'base64'",
                other
            ));
        }
    };

    output.insert(format!("{}_content", output_key), body_value);
    output.insert(
        format!("{}_encoding", output_key),
        serde_json::Value::String(encoding.to_string()),
    );
    output.insert(
        format!("{}_size", output_key),
        serde_json::Value::Number(bytes.len().into()),
    );
    Ok(())
}

pub struct S3PutObjectNode;

#[async_trait]
impl Node for S3PutObjectNode {
    fn node_type(&self) -> &str {
        "s3_put_object"
    }

    fn description(&self) -> &str {
        "Upload an object to S3 (or S3-compatible storage) from text or base64 input"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let bucket =
            resolve_required(config, "bucket", Some("S3_BUCKET"), &ctx).ok_or_else(|| {
                anyhow::anyhow!("s3_put_object requires 'bucket' or S3_BUCKET env var")
            })?;
        let key = resolve_required(config, "key", None, &ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_put_object requires 'key'"))?;
        let output_key = resolve_output_key(config);
        let content_type = resolve_optional(config, "content_type", None, &ctx)
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let body = resolve_payload_bytes(config, &ctx).await?;
        let client = build_s3_client(config, &ctx).await?;
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

pub struct S3PresignUrlNode;

#[async_trait]
impl Node for S3PresignUrlNode {
    fn node_type(&self) -> &str {
        "s3_presign_url"
    }

    fn description(&self) -> &str {
        "Generate a presigned URL for a supported S3 operation"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let bucket =
            resolve_required(config, "bucket", Some("S3_BUCKET"), &ctx).ok_or_else(|| {
                anyhow::anyhow!("s3_presign_url requires 'bucket' or S3_BUCKET env var")
            })?;
        let key = resolve_required(config, "key", None, &ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_presign_url requires 'key'"))?;
        let method = config
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or("GET")
            .to_ascii_uppercase();
        let expires_in = resolve_expires_in(config)?;
        let output_key = resolve_output_key(config);
        let content_type = resolve_optional(config, "content_type", None, &ctx)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let content_length = resolve_content_length(config);

        let client = build_s3_client(config, &ctx).await?;
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

pub struct S3GetObjectNode;

#[async_trait]
impl Node for S3GetObjectNode {
    fn node_type(&self) -> &str {
        "s3_get_object"
    }

    fn description(&self) -> &str {
        "Download an object from S3"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let bucket =
            resolve_required(config, "bucket", Some("S3_BUCKET"), &ctx).ok_or_else(|| {
                anyhow::anyhow!("s3_get_object requires 'bucket' or S3_BUCKET env var")
            })?;
        let key = resolve_required(config, "key", None, &ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_get_object requires 'key'"))?;
        let output_key = resolve_output_key(config);
        let output_encoding = config
            .get("encoding")
            .and_then(|value| value.as_str())
            .unwrap_or("text");

        let client = build_s3_client(config, &ctx).await?;
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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let bucket =
            resolve_required(config, "bucket", Some("S3_BUCKET"), &ctx).ok_or_else(|| {
                anyhow::anyhow!("s3_delete_object requires 'bucket' or S3_BUCKET env var")
            })?;
        let key = resolve_required(config, "key", None, &ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_delete_object requires 'key'"))?;
        let output_key = resolve_output_key(config);
        let version_id = resolve_optional(config, "version_id", None, &ctx);

        let client = build_s3_client(config, &ctx).await?;
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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_bucket = resolve_required(config, "source_bucket", Some("S3_BUCKET"), &ctx)
            .ok_or_else(|| {
                anyhow::anyhow!("s3_copy_object requires 'source_bucket' or S3_BUCKET env var")
            })?;
        let source_key = resolve_required(config, "source_key", None, &ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_copy_object requires 'source_key'"))?;
        let destination_bucket = resolve_required(config, "bucket", Some("S3_BUCKET"), &ctx)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "s3_copy_object requires 'bucket' (destination bucket) or S3_BUCKET env var"
                )
            })?;
        let destination_key = resolve_required(config, "key", None, &ctx)
            .ok_or_else(|| anyhow::anyhow!("s3_copy_object requires destination 'key'"))?;
        let output_key = resolve_output_key(config);
        let copy_source = format!("{}/{}", source_bucket, source_key);

        let client = build_s3_client(config, &ctx).await?;
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

pub struct S3ListObjectsNode;

#[async_trait]
impl Node for S3ListObjectsNode {
    fn node_type(&self) -> &str {
        "s3_list_objects"
    }

    fn description(&self) -> &str {
        "List objects under a S3 key prefix"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let bucket =
            resolve_required(config, "bucket", Some("S3_BUCKET"), &ctx).ok_or_else(|| {
                anyhow::anyhow!("s3_list_objects requires 'bucket' or S3_BUCKET env var")
            })?;
        let prefix = resolve_optional(config, "prefix", None, &ctx).unwrap_or_default();
        let delimiter = config
            .get("delimiter")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let output_key = resolve_output_key(config);
        let max_keys = config.get("max_keys").and_then(|value| value.as_u64());
        let max_keys = max_keys.and_then(|value| i32::try_from(value).ok());

        let client = build_s3_client(config, &ctx).await?;

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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let client = build_s3_client(config, &ctx).await?;
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
