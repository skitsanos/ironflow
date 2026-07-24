use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3vectors::operation::query_vectors::builders::QueryVectorsInputBuilder;
use aws_sdk_s3vectors::types::{DistanceMetric, VectorData};

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::node_config::config_bool;

use super::client::build_s3vector_client;
use super::config::{resolve_optional, resolve_output_key};
use super::document::{document_to_json, parse_metadata};
use super::parameters::{resolve_f64, resolve_u32};
use super::target::{IndexTarget, TargetPolicy, resolve_index_target};
use super::vectors::resolve_query_vector;

struct PreparedQueryVectors {
    request: QueryVectorsInputBuilder,
    return_metadata: bool,
    return_distance: bool,
    min_similarity: Option<f64>,
    strict: bool,
}

fn prepare_query_vectors_input(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<PreparedQueryVectors> {
    let target = resolve_index_target(
        config,
        ctx,
        "s3vector_query_vectors",
        TargetPolicy::AllowEnvironment,
    )?;
    let top_k = resolve_u32(config, &["top_k"], ctx, "s3vector_query_vectors", "top_k")?;
    if top_k == 0 {
        anyhow::bail!("s3vector_query_vectors requires 'top_k' to be greater than zero");
    }
    let query_vector = resolve_query_vector(config, ctx, "s3vector_query_vectors")?;
    let return_metadata = config_bool(config, "return_metadata", ctx).unwrap_or(false);
    let return_distance = config_bool(config, "return_distance", ctx).unwrap_or(false);
    let min_similarity = if config.get("min_similarity").is_some() {
        let value = resolve_f64(config, ctx, "s3vector_query_vectors", "min_similarity")?;
        if !(0.0..=1.0).contains(&value) {
            anyhow::bail!("s3vector_query_vectors requires 'min_similarity' to be between 0 and 1");
        }
        Some(value)
    } else {
        None
    };
    let strict = config_bool(config, "strict", ctx).unwrap_or(false);
    let filter = if let Some(filter_value) = config.get("filter") {
        Some(parse_metadata(filter_value, "s3vector_query_vectors")?)
    } else {
        let source_key = resolve_optional(config, &["filter_key"], &[], ctx);
        source_key
            .and_then(|value| ctx.get(&value).cloned())
            .map(|value| parse_metadata(&value, "s3vector_query_vectors"))
            .transpose()?
    };

    let request = QueryVectorsInputBuilder::default()
        .top_k(top_k as i32)
        .query_vector(VectorData::Float32(query_vector))
        .return_metadata(return_metadata)
        .return_distance(return_distance || min_similarity.is_some());
    let request = match target {
        IndexTarget::Names {
            bucket_name,
            index_name,
        } => request
            .vector_bucket_name(bucket_name)
            .index_name(index_name),
        IndexTarget::Arn(arn) => request.index_arn(arn),
    };
    let request = match filter {
        Some(filter) => request.filter(filter),
        None => request,
    };

    Ok(PreparedQueryVectors {
        request,
        return_metadata,
        return_distance,
        min_similarity,
        strict,
    })
}

pub struct S3VectorQueryVectorsNode;

#[async_trait]
impl Node for S3VectorQueryVectorsNode {
    fn node_type(&self) -> &str {
        "s3vector_query_vectors"
    }

    fn description(&self) -> &str {
        "Query an S3 Vector index by vector similarity"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let PreparedQueryVectors {
            request,
            return_metadata,
            return_distance,
            min_similarity,
            strict,
        } = prepare_query_vectors_input(config, ctx)?;
        let client = build_s3vector_client(config, ctx).await?;
        let response = request.send_with(&client).await?;
        let distance_metric = response.distance_metric();
        let should_apply_min_similarity = if min_similarity.is_some() && strict {
            if distance_metric != Some(&DistanceMetric::Cosine) {
                anyhow::bail!(
                    "s3vector_query_vectors min_similarity requires cosine distance metric when strict=true"
                );
            }
            true
        } else {
            min_similarity.is_some() && distance_metric == Some(&DistanceMetric::Cosine)
        };
        let min_similarity_value = min_similarity.unwrap_or_default();
        let vectors: Vec<serde_json::Value> = response
            .vectors()
            .iter()
            .filter_map(|vector| {
                if should_apply_min_similarity {
                    let distance = vector.distance()?;
                    let similarity = 1.0_f64 - f64::from(distance);
                    if similarity < min_similarity_value {
                        return None;
                    }
                }

                let mut item = serde_json::Map::new();
                item.insert("key".to_string(), serde_json::json!(vector.key()));
                if return_distance && let Some(distance) = vector.distance() {
                    item.insert("distance".to_string(), serde_json::json!(distance));
                }
                if return_metadata && let Some(metadata) = vector.metadata() {
                    item.insert("metadata".to_string(), document_to_json(metadata));
                }
                Some(serde_json::Value::Object(item))
            })
            .collect();
        let mut output = NodeOutput::new();
        if let Some(distance_metric) = distance_metric {
            output.insert(
                format!("{}_distance_metric", output_key),
                serde_json::Value::String(distance_metric.as_str().to_string()),
            );
        }
        if let Some(min_similarity) = min_similarity {
            output.insert(
                format!("{}_min_similarity", output_key),
                serde_json::json!(min_similarity),
            );
            output.insert(
                format!("{}_min_similarity_applied", output_key),
                serde_json::json!(should_apply_min_similarity),
            );
        }
        output.insert(
            format!("{}_count", output_key),
            serde_json::json!(vectors.len()),
        );
        output.insert(
            format!("{}_vectors", output_key),
            serde_json::Value::Array(vectors),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

#[cfg(test)]
mod tests;
