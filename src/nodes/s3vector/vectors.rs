use anyhow::Result;
use aws_sdk_s3vectors::types::{PutInputVector, VectorData};

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_ctx;

use super::config::resolve_optional;
use super::document::parse_metadata;

pub(super) fn resolve_float_vector(
    value: &serde_json::Value,
    field: &str,
    node: &str,
) -> Result<Vec<f32>> {
    resolve_float_vector_value(value, node, field)
}

pub(super) fn resolve_float_vector_value(
    value: &serde_json::Value,
    node: &str,
    field: &str,
) -> Result<Vec<f32>> {
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{} requires '{}' to be an array", node, field))?;

    if values.is_empty() {
        anyhow::bail!("{} requires '{}' to be a non-empty array", node, field);
    }

    values
        .iter()
        .map(|value| {
            let number = value.as_f64().ok_or_else(|| {
                anyhow::anyhow!("{} requires '{}' items to be numbers", node, field)
            })?;
            if !number.is_finite() {
                anyhow::bail!("{} requires '{}' values to be finite numbers", node, field);
            }
            Ok(number as f32)
        })
        .collect()
}

pub(super) fn resolve_query_vector(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
) -> Result<Vec<f32>> {
    if let Some(query_vector) = config.get("query_vector") {
        return resolve_float_vector_value(query_vector, node, "query_vector");
    }

    let query_vector_key = config
        .get("query_vector_key")
        .and_then(|value| value.as_str());
    if query_vector_key.is_none() {
        anyhow::bail!("s3vector_query_vectors requires 'query_vector' or 'query_vector_key'");
    }

    let query_vector_key = query_vector_key.unwrap_or_default();
    let key = interpolate_ctx(query_vector_key, ctx);
    let vector_value = ctx
        .get(&key)
        .ok_or_else(|| anyhow::anyhow!("s3vector_query_vectors requires '{}' in context", key))?;

    resolve_float_vector_value(vector_value, node, "query_vector_key")
}

pub(super) fn resolve_vectors_data(
    config: &serde_json::Value,
    ctx: &Context,
    node: &str,
) -> Result<Vec<PutInputVector>> {
    let vectors = if let Some(vectors) = config.get("vectors") {
        vectors
    } else {
        let source_key =
            resolve_optional(config, &["vectors_source_key"], &[], ctx).ok_or_else(|| {
                anyhow::anyhow!("{} requires 'vectors' array or 'vectors_source_key'", node)
            })?;
        ctx.get(&source_key).ok_or_else(|| {
            anyhow::anyhow!(
                "{} requires source key '{}' to exist in context",
                node,
                source_key
            )
        })?
    };

    let vectors = vectors
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{} requires 'vectors' to be an array", node))?;

    if vectors.is_empty() {
        anyhow::bail!("{} requires at least one vector in 'vectors'", node);
    }

    vectors
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let vector = value.as_object().ok_or_else(|| {
                anyhow::anyhow!("{} expects each vector entry to be an object", node)
            })?;

            let key = vector
                .get("key")
                .and_then(|value| value.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!("{} requires vector[{}].key to be a string", node, index)
                })?;

            let data = vector.get("data").ok_or_else(|| {
                anyhow::anyhow!(
                    "{} requires vector[{}].data to be an array of numbers",
                    node,
                    index
                )
            })?;

            let mut builder = PutInputVector::builder()
                .key(interpolate_ctx(key, ctx))
                .data(VectorData::Float32(resolve_float_vector(
                    data, "data", node,
                )?));

            if let Some(metadata) = vector.get("metadata") {
                builder = builder.metadata(parse_metadata(metadata, node)?);
            }

            Ok(builder.build()?)
        })
        .collect()
}
