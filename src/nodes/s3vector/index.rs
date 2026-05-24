use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::{resolve_bucket_id, resolve_index_id, resolve_output_key};
use super::document::{parse_data_type, parse_distance_metric};
use super::parameters::{resolve_non_empty_string, resolve_u32};

pub struct S3VectorCreateIndexNode;

#[async_trait]
impl Node for S3VectorCreateIndexNode {
    fn node_type(&self) -> &str {
        "s3vector_create_index"
    }

    fn description(&self) -> &str {
        "Create an S3 Vector index"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (bucket_name, bucket_arn) = resolve_bucket_id(config, ctx, "s3vector_create_index")?;
        if bucket_name.is_none() && bucket_arn.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_create_index requires 'vector_bucket_name' or 'vector_bucket_arn'"
            ));
        }

        let index_name = resolve_non_empty_string(
            config,
            &["index_name", "index"],
            &["S3VECTOR_INDEX_NAME"],
            ctx,
            "s3vector_create_index",
            "index_name",
        )?;
        let data_type = parse_data_type(
            config
                .get("data_type")
                .ok_or_else(|| anyhow::anyhow!("s3vector_create_index requires 'data_type'"))?,
            "s3vector_create_index",
        )?;
        let distance_metric = parse_distance_metric(
            config.get("distance_metric").ok_or_else(|| {
                anyhow::anyhow!("s3vector_create_index requires 'distance_metric'")
            })?,
            "s3vector_create_index",
        )?;
        let dimension = resolve_u32(config, &["dimension"], "s3vector_create_index", "dimension")?;
        if dimension == 0 {
            anyhow::bail!("s3vector_create_index requires 'dimension' to be greater than zero");
        }

        let mut request = build_s3vector_client(config, ctx)
            .await?
            .create_index()
            .data_type(data_type.clone())
            .distance_metric(distance_metric.clone())
            .index_name(index_name.clone())
            .dimension(dimension as i32);

        if let Some(bucket_name) = bucket_name.clone() {
            request = request.vector_bucket_name(bucket_name);
        } else if let Some(bucket_arn) = bucket_arn.clone() {
            request = request.vector_bucket_arn(bucket_arn);
        }

        let response = request.send().await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_index_name", output_key),
            serde_json::Value::String(index_name),
        );
        if let Some(bucket_name) = &bucket_name {
            output.insert(
                format!("{}_bucket_name", output_key),
                serde_json::Value::String(bucket_name.clone()),
            );
        }
        if let Some(bucket_arn) = &bucket_arn {
            output.insert(
                format!("{}_bucket_arn", output_key),
                serde_json::Value::String(bucket_arn.clone()),
            );
        }
        output.insert(
            format!("{}_distance_metric", output_key),
            serde_json::Value::String(distance_metric.as_str().to_string()),
        );
        output.insert(
            format!("{}_data_type", output_key),
            serde_json::Value::String(data_type.as_str().to_string()),
        );
        output.insert(
            format!("{}_dimension", output_key),
            serde_json::json!(dimension),
        );
        if let Some(index_arn) = response.index_arn() {
            output.insert(
                format!("{}_index_arn", output_key),
                serde_json::Value::String(index_arn.to_string()),
            );
        }
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct S3VectorGetIndexNode;

#[async_trait]
impl Node for S3VectorGetIndexNode {
    fn node_type(&self) -> &str {
        "s3vector_get_index"
    }

    fn description(&self) -> &str {
        "Get metadata for an S3 Vector index"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (bucket_name, _bucket_arn) = resolve_bucket_id(config, ctx, "s3vector_get_index")?;
        let (index_name, index_arn) = resolve_index_id(config, ctx, "s3vector_get_index")?;
        if index_name.is_none() && index_arn.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_get_index requires 'index_name' or 'index_arn'"
            ));
        }
        if index_name.is_some() && bucket_name.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_get_index requires a vector bucket when using 'index_name'"
            ));
        }

        let mut request = build_s3vector_client(config, ctx).await?.get_index();

        if let Some(bucket_name) = bucket_name {
            request = request.vector_bucket_name(bucket_name);
        }
        if let Some(index_name) = index_name.clone() {
            request = request.index_name(index_name);
        } else if let Some(index_arn) = index_arn.clone() {
            request = request.index_arn(index_arn);
        }

        let response = request.send().await?;
        let index = response.index();

        let mut output = NodeOutput::new();
        if let Some(index) = index {
            output.insert(
                format!("{}_index_name", output_key),
                serde_json::Value::String(index.index_name().to_string()),
            );
            output.insert(
                format!("{}_index_arn", output_key),
                serde_json::Value::String(index.index_arn().to_string()),
            );
            output.insert(
                format!("{}_bucket_name", output_key),
                serde_json::Value::String(index.vector_bucket_name().to_string()),
            );
            output.insert(
                format!("{}_dimension", output_key),
                serde_json::json!(index.dimension()),
            );
            output.insert(
                format!("{}_distance_metric", output_key),
                serde_json::Value::String(index.distance_metric().as_str().to_string()),
            );
            output.insert(
                format!("{}_data_type", output_key),
                serde_json::Value::String(index.data_type().as_str().to_string()),
            );
            output.insert(
                format!("{}_creation_time", output_key),
                serde_json::Value::String(index.creation_time().to_string()),
            );
            if let Some(metadata_configuration) = index.metadata_configuration() {
                output.insert(
                    format!("{}_metadata_configuration", output_key),
                    serde_json::json!(format!("{:?}", metadata_configuration)),
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
