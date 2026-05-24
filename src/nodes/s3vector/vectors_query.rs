use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3vectors::types::{DistanceMetric, VectorData};

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::{resolve_bucket_id, resolve_index_id, resolve_optional, resolve_output_key};
use super::document::{document_to_json, parse_metadata};
use super::parameters::{resolve_f64, resolve_u32};
use super::vectors::resolve_query_vector;

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
        let (bucket_name, _bucket_arn) = resolve_bucket_id(config, ctx, "s3vector_query_vectors")?;
        let (index_name, index_arn) = resolve_index_id(config, ctx, "s3vector_query_vectors")?;
        if index_name.is_none() && index_arn.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_query_vectors requires 'index_name' or 'index_arn'"
            ));
        }
        if index_name.is_some() && bucket_name.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_query_vectors requires a vector bucket when using 'index_name'"
            ));
        }

        let top_k = resolve_u32(config, &["top_k"], "s3vector_query_vectors", "top_k")?;
        if top_k == 0 {
            anyhow::bail!("s3vector_query_vectors requires 'top_k' to be greater than zero");
        }

        let query_vector = resolve_query_vector(config, ctx, "s3vector_query_vectors")?;

        let return_metadata = config
            .get("return_metadata")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let return_distance = config
            .get("return_distance")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let min_similarity = if config.get("min_similarity").is_some() {
            let min_similarity = resolve_f64(config, "s3vector_query_vectors", "min_similarity")?;
            if !(0.0..=1.0).contains(&min_similarity) {
                anyhow::bail!(
                    "s3vector_query_vectors requires 'min_similarity' to be between 0 and 1"
                );
            }
            Some(min_similarity)
        } else {
            None
        };
        let strict = config
            .get("strict")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let should_return_distance = return_distance || min_similarity.is_some();

        let filter = if let Some(filter_value) = config.get("filter") {
            Some(parse_metadata(filter_value, "s3vector_query_vectors")?)
        } else {
            let source_key = resolve_optional(config, &["filter_key"], &[], ctx);
            source_key
                .and_then(|value| ctx.get(&value).cloned())
                .map(|value| parse_metadata(&value, "s3vector_query_vectors"))
                .transpose()?
        };

        let mut request = build_s3vector_client(config, ctx)
            .await?
            .query_vectors()
            .top_k(top_k as i32)
            .query_vector(VectorData::Float32(query_vector))
            .return_metadata(return_metadata)
            .return_distance(should_return_distance);
        if let Some(bucket_name) = bucket_name {
            request = request.vector_bucket_name(bucket_name);
        }
        if let Some(index_name) = index_name {
            request = request.index_name(index_name);
        }
        if let Some(index_arn) = index_arn {
            request = request.index_arn(index_arn);
        }
        if let Some(filter) = filter {
            request = request.filter(filter);
        }

        let response = request.send().await?;
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
                    if let Some(distance) = vector.distance() {
                        let similarity = 1.0_f64 - f64::from(distance);
                        if similarity < min_similarity_value {
                            return None;
                        }
                    } else {
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
