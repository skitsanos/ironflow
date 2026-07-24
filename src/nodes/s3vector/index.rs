use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3vectors::operation::create_index::builders::CreateIndexInputBuilder;
use aws_sdk_s3vectors::operation::get_index::builders::GetIndexInputBuilder;
use aws_sdk_s3vectors::types::{DataType, DistanceMetric};

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::resolve_output_key;
use super::document::{parse_data_type, parse_distance_metric};
use super::parameters::resolve_u32;
use super::target::{
    BucketTarget, CreateIndexTarget, IndexTarget, TargetPolicy, resolve_create_index_target,
    resolve_index_target,
};

struct PreparedCreateIndex {
    request: CreateIndexInputBuilder,
    bucket_name: Option<String>,
    bucket_arn: Option<String>,
    index_name: String,
    data_type: DataType,
    distance_metric: DistanceMetric,
    dimension: u32,
}

fn prepare_create_index_input(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<PreparedCreateIndex> {
    let CreateIndexTarget { bucket, index_name } =
        resolve_create_index_target(config, ctx, "s3vector_create_index")?;
    let data_type = parse_data_type(
        config
            .get("data_type")
            .ok_or_else(|| anyhow::anyhow!("s3vector_create_index requires 'data_type'"))?,
        "s3vector_create_index",
    )?;
    let distance_metric = parse_distance_metric(
        config
            .get("distance_metric")
            .ok_or_else(|| anyhow::anyhow!("s3vector_create_index requires 'distance_metric'"))?,
        "s3vector_create_index",
    )?;
    let dimension = resolve_u32(
        config,
        &["dimension"],
        ctx,
        "s3vector_create_index",
        "dimension",
    )?;
    if dimension == 0 {
        anyhow::bail!("s3vector_create_index requires 'dimension' to be greater than zero");
    }

    let request = CreateIndexInputBuilder::default()
        .data_type(data_type.clone())
        .distance_metric(distance_metric.clone())
        .index_name(index_name.clone())
        .dimension(dimension as i32);
    let (request, bucket_name, bucket_arn) = match bucket {
        BucketTarget::Name(name) => (request.vector_bucket_name(name.clone()), Some(name), None),
        BucketTarget::Arn(arn) => (request.vector_bucket_arn(arn.clone()), None, Some(arn)),
    };

    Ok(PreparedCreateIndex {
        request,
        bucket_name,
        bucket_arn,
        index_name,
        data_type,
        distance_metric,
        dimension,
    })
}

fn prepare_get_index_input(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<GetIndexInputBuilder> {
    let target = resolve_index_target(
        config,
        ctx,
        "s3vector_get_index",
        TargetPolicy::AllowEnvironment,
    )?;
    let request = match target {
        IndexTarget::Names {
            bucket_name,
            index_name,
        } => GetIndexInputBuilder::default()
            .vector_bucket_name(bucket_name)
            .index_name(index_name),
        IndexTarget::Arn(arn) => GetIndexInputBuilder::default().index_arn(arn),
    };
    Ok(request)
}

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
        let prepared = prepare_create_index_input(config, ctx)?;
        let client = build_s3vector_client(config, ctx).await?;
        let response = prepared.request.send_with(&client).await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_index_name", output_key),
            serde_json::Value::String(prepared.index_name),
        );
        if let Some(bucket_name) = prepared.bucket_name {
            output.insert(
                format!("{}_bucket_name", output_key),
                serde_json::Value::String(bucket_name),
            );
        }
        if let Some(bucket_arn) = prepared.bucket_arn {
            output.insert(
                format!("{}_bucket_arn", output_key),
                serde_json::Value::String(bucket_arn),
            );
        }
        output.insert(
            format!("{}_distance_metric", output_key),
            serde_json::Value::String(prepared.distance_metric.as_str().to_string()),
        );
        output.insert(
            format!("{}_data_type", output_key),
            serde_json::Value::String(prepared.data_type.as_str().to_string()),
        );
        output.insert(
            format!("{}_dimension", output_key),
            serde_json::json!(prepared.dimension),
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
        let request = prepare_get_index_input(config, ctx)?;
        let client = build_s3vector_client(config, ctx).await?;
        let response = request.send_with(&client).await?;
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

#[cfg(test)]
mod tests;
