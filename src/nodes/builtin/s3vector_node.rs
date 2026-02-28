use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3vectors::Client;
use aws_sdk_s3vectors::config::Region;
use aws_sdk_s3vectors::types::{DataType, DistanceMetric, PutInputVector, VectorData};
use aws_smithy_types::Document;
use aws_smithy_types::Number;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

fn resolve_optional(
    config: &serde_json::Value,
    keys: &[&str],
    env_keys: &[&str],
    ctx: &Context,
) -> Option<String> {
    keys.iter()
        .find_map(|key| {
            config
                .get(key)
                .and_then(|value| value.as_str())
                .map(|value| interpolate_ctx(value, ctx))
        })
        .or_else(|| env_keys.iter().find_map(|key| std::env::var(key).ok()))
}

fn resolve_required(
    config: &serde_json::Value,
    keys: &[&str],
    env_keys: &[&str],
    ctx: &Context,
    node: &str,
    field: &str,
) -> Result<String> {
    resolve_optional(config, keys, env_keys, ctx).ok_or_else(|| {
        let env_description = env_keys
            .first()
            .map(|value| format!(" or {} env var", value))
            .unwrap_or_default();
        anyhow::anyhow!("{} requires '{}'{}", node, field, env_description)
    })
}

fn resolve_output_key(config: &serde_json::Value) -> String {
    config
        .get("output_key")
        .and_then(|value| value.as_str())
        .unwrap_or("s3vector")
        .to_string()
}

fn resolve_region(config: &serde_json::Value, ctx: &Context) -> Option<String> {
    resolve_optional(
        config,
        &["region"],
        &[
            "S3VECTORS_REGION",
            "S3_REGION",
            "AWS_REGION",
            "AWS_DEFAULT_REGION",
        ],
        ctx,
    )
}

fn resolve_endpoint_url(config: &serde_json::Value, ctx: &Context) -> Option<String> {
    resolve_optional(config, &["endpoint_url"], &["AWS_ENDPOINT_URL"], ctx)
}

fn resolve_i64(config: &serde_json::Value, keys: &[&str], node: &str, field: &str) -> Result<i64> {
    let mut value = config.get(keys[0]);
    if value.is_none() {
        for key in &keys[1..] {
            if let Some(candidate) = config.get(key) {
                value = Some(candidate);
                break;
            }
        }
    }
    if let Some(value) = value {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| {
                value.as_f64().and_then(|value| {
                    if (value.trunc() == value) && value.is_finite() {
                        Some(value as i64)
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| anyhow::anyhow!("{} requires '{}' as an integer", node, field))
    } else {
        Err(anyhow::anyhow!("{} requires '{}' field", node, field))
    }
}

fn resolve_u32(config: &serde_json::Value, keys: &[&str], node: &str, field: &str) -> Result<u32> {
    resolve_i64(config, keys, node, field).and_then(|value| {
        u32::try_from(value)
            .map_err(|_| anyhow::anyhow!("{} requires '{}' as a non-negative integer", node, field))
    })
}

fn resolve_non_empty_string(
    config: &serde_json::Value,
    keys: &[&str],
    env_keys: &[&str],
    ctx: &Context,
    node: &str,
    field: &str,
) -> Result<String> {
    let value = resolve_required(config, keys, env_keys, ctx, node, field)?;
    if value.is_empty() {
        anyhow::bail!("{} requires '{}' to be non-empty", node, field);
    }
    Ok(value)
}

fn resolve_string_array(
    config: &serde_json::Value,
    primary_key: &str,
    fallback_key: Option<&str>,
    ctx: &Context,
    node: &str,
    field: &str,
) -> Result<Vec<String>> {
    let raw = if let Some(values) = config.get(primary_key) {
        values
    } else if let Some(fallback_key) = fallback_key {
        let fallback = resolve_optional(config, &[fallback_key], &[], ctx)
            .ok_or_else(|| anyhow::anyhow!("{} requires '{}'", node, field))?;
        ctx.get(&fallback).ok_or_else(|| {
            anyhow::anyhow!(
                "{} requires '{}' source key '{}' to exist in context",
                node,
                field,
                fallback
            )
        })?
    } else {
        return Err(anyhow::anyhow!("{} requires '{}'", node, field));
    };

    let raw = raw
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{} requires '{}' to be an array", node, field))?;

    let mut values = Vec::new();
    for value in raw {
        let item = value.as_str().ok_or_else(|| {
            anyhow::anyhow!("{} requires each '{}' item to be a string", node, field)
        })?;
        values.push(interpolate_ctx(item, ctx));
    }

    if values.is_empty() {
        anyhow::bail!("{} requires '{}' to be a non-empty array", node, field);
    }

    Ok(values)
}

fn resolve_float_vector(value: &serde_json::Value, field: &str, node: &str) -> Result<Vec<f32>> {
    resolve_float_vector_value(value, node, field)
}

fn resolve_float_vector_value(
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

fn resolve_query_vector(config: &serde_json::Value, ctx: &Context, node: &str) -> Result<Vec<f32>> {
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

fn parse_json_to_document(value: &serde_json::Value) -> Result<Document> {
    match value {
        serde_json::Value::Null => Ok(Document::Null),
        serde_json::Value::Bool(value) => Ok(Document::from(*value)),
        serde_json::Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Ok(Document::from(value))
            } else if let Some(value) = number.as_u64() {
                Ok(Document::from(value))
            } else if let Some(value) = number.as_f64() {
                if !value.is_finite() {
                    anyhow::bail!("JSON to Document conversion requires finite numbers only");
                }
                Ok(Document::from(value))
            } else {
                Err(anyhow::anyhow!("unsupported JSON number format"))
            }
        }
        serde_json::Value::String(value) => Ok(Document::from(value.clone())),
        serde_json::Value::Array(values) => {
            let mut items = Vec::with_capacity(values.len());
            for value in values {
                items.push(parse_json_to_document(value)?);
            }
            Ok(Document::from(items))
        }
        serde_json::Value::Object(entries) => {
            let mut object = std::collections::HashMap::new();
            for (key, value) in entries {
                object.insert(key.clone(), parse_json_to_document(value)?);
            }
            Ok(Document::from(object))
        }
    }
}

fn parse_metadata(value: &serde_json::Value, _node: &str) -> Result<Document> {
    let doc = parse_json_to_document(value)?;
    match &doc {
        Document::Object(_)
        | Document::Array(_)
        | Document::Number(_)
        | Document::String(_)
        | Document::Bool(_)
        | Document::Null => Ok(doc),
    }
}

fn document_to_json(value: &Document) -> serde_json::Value {
    match value {
        Document::Null => serde_json::Value::Null,
        Document::Bool(v) => serde_json::Value::Bool(*v),
        Document::String(v) => serde_json::Value::String(v.clone()),
        Document::Number(number) => match number {
            Number::PosInt(number) => serde_json::Number::from(*number).into(),
            Number::NegInt(number) => serde_json::Number::from(*number).into(),
            Number::Float(number) => serde_json::Number::from_f64(*number)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
        },
        Document::Object(entries) => {
            let mut map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            for (key, value) in entries {
                map.insert(key.clone(), document_to_json(value));
            }
            serde_json::Value::Object(map)
        }
        Document::Array(values) => {
            let items = values.iter().map(document_to_json).collect();
            serde_json::Value::Array(items)
        }
    }
}

fn parse_data_type(value: &serde_json::Value, node: &str) -> Result<DataType> {
    let value = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("{} requires 'data_type' as string", node))?;
    DataType::try_parse(value)
        .map_err(|_| anyhow::anyhow!("{} received unsupported data_type '{}'", node, value))
}

fn parse_distance_metric(value: &serde_json::Value, node: &str) -> Result<DistanceMetric> {
    let value = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("{} requires 'distance_metric' as string", node))?;
    DistanceMetric::try_parse(value)
        .map_err(|_| anyhow::anyhow!("{} received unsupported distance_metric '{}'", node, value))
}

fn resolve_bucket_id(
    config: &serde_json::Value,
    ctx: &Context,
    _node: &str,
) -> Result<(Option<String>, Option<String>)> {
    let name = resolve_optional(
        config,
        &["vector_bucket_name", "bucket"],
        &["S3VECTOR_BUCKET_NAME", "S3_BUCKET"],
        ctx,
    );
    let arn = resolve_optional(
        config,
        &["vector_bucket_arn"],
        &["S3VECTOR_BUCKET_ARN"],
        ctx,
    );
    Ok((name, arn))
}

fn resolve_index_id(
    config: &serde_json::Value,
    ctx: &Context,
    _node: &str,
) -> Result<(Option<String>, Option<String>)> {
    let name = resolve_optional(
        config,
        &["index_name", "index"],
        &["S3VECTOR_INDEX_NAME"],
        ctx,
    );
    let arn = resolve_optional(config, &["index_arn"], &["S3VECTOR_INDEX_ARN"], ctx);
    Ok((name, arn))
}

fn resolve_vectors_data(
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

async fn build_s3vector_client(config: &serde_json::Value, ctx: &Context) -> Result<Client> {
    let region = resolve_region(config, ctx);
    let endpoint_url = resolve_endpoint_url(config, ctx);

    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(region) = region {
        loader = loader.region(Region::new(region));
    }

    let base_config = loader.load().await;
    let mut builder = aws_sdk_s3vectors::config::Builder::from(&base_config);
    if let Some(endpoint_url) = endpoint_url {
        builder = builder.endpoint_url(endpoint_url);
    }
    Ok(Client::from_conf(builder.build()))
}

pub struct S3VectorCreateBucketNode;

#[async_trait]
impl Node for S3VectorCreateBucketNode {
    fn node_type(&self) -> &str {
        "s3vector_create_bucket"
    }

    fn description(&self) -> &str {
        "Create an S3 Vector bucket"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let bucket_name = resolve_non_empty_string(
            config,
            &["vector_bucket_name", "bucket"],
            &["S3VECTOR_BUCKET_NAME", "S3_BUCKET"],
            &ctx,
            "s3vector_create_bucket",
            "vector_bucket_name",
        )?;
        let output_key = resolve_output_key(config);

        let client = build_s3vector_client(config, &ctx).await?;
        let response = client
            .create_vector_bucket()
            .vector_bucket_name(bucket_name.clone())
            .send()
            .await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_bucket_name", output_key),
            serde_json::Value::String(bucket_name),
        );
        if let Some(arn) = response.vector_bucket_arn() {
            output.insert(
                format!("{}_bucket_arn", output_key),
                serde_json::Value::String(arn.to_string()),
            );
        }
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct S3VectorGetBucketNode;

#[async_trait]
impl Node for S3VectorGetBucketNode {
    fn node_type(&self) -> &str {
        "s3vector_get_bucket"
    }

    fn description(&self) -> &str {
        "Get metadata for an S3 Vector bucket"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (bucket_name, bucket_arn) = resolve_bucket_id(config, &ctx, "s3vector_get_bucket")?;
        if bucket_name.is_none() && bucket_arn.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_get_bucket requires 'vector_bucket_name' or 'vector_bucket_arn'"
            ));
        }

        let mut request = build_s3vector_client(config, &ctx)
            .await?
            .get_vector_bucket();

        if let Some(name) = bucket_name {
            request = request.vector_bucket_name(name);
        }
        if let Some(arn) = bucket_arn {
            request = request.vector_bucket_arn(arn);
        }

        let response = request.send().await?;
        let bucket = response.vector_bucket();

        let mut output = NodeOutput::new();
        if let Some(bucket) = bucket {
            output.insert(
                format!("{}_bucket_name", output_key),
                serde_json::Value::String(bucket.vector_bucket_name().to_string()),
            );
            output.insert(
                format!("{}_bucket_arn", output_key),
                serde_json::Value::String(bucket.vector_bucket_arn().to_string()),
            );
            output.insert(
                format!("{}_creation_time", output_key),
                serde_json::Value::String(bucket.creation_time().to_string()),
            );
            if let Some(encryption_configuration) = bucket.encryption_configuration() {
                output.insert(
                    format!("{}_encryption_configuration", output_key),
                    serde_json::json!(format!("{:?}", encryption_configuration)),
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

pub struct S3VectorCreateIndexNode;

#[async_trait]
impl Node for S3VectorCreateIndexNode {
    fn node_type(&self) -> &str {
        "s3vector_create_index"
    }

    fn description(&self) -> &str {
        "Create an S3 Vector index"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (bucket_name, bucket_arn) = resolve_bucket_id(config, &ctx, "s3vector_create_index")?;
        if bucket_name.is_none() && bucket_arn.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_create_index requires 'vector_bucket_name' or 'vector_bucket_arn'"
            ));
        }

        let index_name = resolve_non_empty_string(
            config,
            &["index_name", "index"],
            &["S3VECTOR_INDEX_NAME"],
            &ctx,
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

        let mut request = build_s3vector_client(config, &ctx)
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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (bucket_name, _bucket_arn) = resolve_bucket_id(config, &ctx, "s3vector_get_index")?;
        let (index_name, index_arn) = resolve_index_id(config, &ctx, "s3vector_get_index")?;
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

        let mut request = build_s3vector_client(config, &ctx).await?.get_index();

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

pub struct S3VectorPutVectorsNode;

#[async_trait]
impl Node for S3VectorPutVectorsNode {
    fn node_type(&self) -> &str {
        "s3vector_put_vectors"
    }

    fn description(&self) -> &str {
        "Upload vectors into an S3 Vector index"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (bucket_name, _bucket_arn) = resolve_bucket_id(config, &ctx, "s3vector_put_vectors")?;
        let (index_name, index_arn) = resolve_index_id(config, &ctx, "s3vector_put_vectors")?;
        if index_name.is_none() && index_arn.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_put_vectors requires 'index_name' or 'index_arn'"
            ));
        }
        if index_name.is_some() && bucket_name.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_put_vectors requires a vector bucket when using 'index_name'"
            ));
        }

        let vectors = resolve_vectors_data(config, &ctx, "s3vector_put_vectors")?;
        let vector_keys: Vec<String> = vectors
            .iter()
            .map(|value| value.key().to_string())
            .collect();

        let mut request = build_s3vector_client(config, &ctx)
            .await?
            .put_vectors()
            .set_vectors(Some(vectors));
        if let Some(bucket_name) = bucket_name {
            request = request.vector_bucket_name(bucket_name);
        }
        if let Some(index_name) = index_name {
            request = request.index_name(index_name);
        }
        if let Some(index_arn) = index_arn {
            request = request.index_arn(index_arn);
        }

        let _response = request.send().await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_vector_count", output_key),
            serde_json::json!(vector_keys.len()),
        );
        output.insert(
            format!("{}_vector_keys", output_key),
            serde_json::Value::Array(
                vector_keys
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (bucket_name, _bucket_arn) = resolve_bucket_id(config, &ctx, "s3vector_query_vectors")?;
        let (index_name, index_arn) = resolve_index_id(config, &ctx, "s3vector_query_vectors")?;
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

        let query_vector = resolve_query_vector(config, &ctx, "s3vector_query_vectors")?;

        let return_metadata = config
            .get("return_metadata")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let return_distance = config
            .get("return_distance")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let filter = if let Some(filter_value) = config.get("filter") {
            Some(parse_metadata(filter_value, "s3vector_query_vectors")?)
        } else {
            let source_key = resolve_optional(config, &["filter_key"], &[], &ctx);
            source_key
                .and_then(|value| ctx.get(&value).cloned())
                .map(|value| parse_metadata(&value, "s3vector_query_vectors"))
                .transpose()?
        };

        let mut request = build_s3vector_client(config, &ctx)
            .await?
            .query_vectors()
            .top_k(top_k as i32)
            .query_vector(VectorData::Float32(query_vector))
            .return_metadata(return_metadata)
            .return_distance(return_distance);
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
        let vectors: Vec<serde_json::Value> = response
            .vectors()
            .iter()
            .map(|vector| {
                let mut item = serde_json::Map::new();
                item.insert("key".to_string(), serde_json::json!(vector.key()));
                if let Some(distance) = vector.distance() {
                    item.insert("distance".to_string(), serde_json::json!(distance));
                }
                if let Some(metadata) = vector.metadata() {
                    item.insert("metadata".to_string(), document_to_json(metadata));
                }
                serde_json::Value::Object(item)
            })
            .collect();

        let mut output = NodeOutput::new();
        if let Some(distance_metric) = response.distance_metric() {
            output.insert(
                format!("{}_distance_metric", output_key),
                serde_json::Value::String(distance_metric.as_str().to_string()),
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

pub struct S3VectorDeleteVectorsNode;

#[async_trait]
impl Node for S3VectorDeleteVectorsNode {
    fn node_type(&self) -> &str {
        "s3vector_delete_vectors"
    }

    fn description(&self) -> &str {
        "Delete vectors from an S3 Vector index"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let (bucket_name, _bucket_arn) =
            resolve_bucket_id(config, &ctx, "s3vector_delete_vectors")?;
        let (index_name, index_arn) = resolve_index_id(config, &ctx, "s3vector_delete_vectors")?;
        if index_name.is_none() && index_arn.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_delete_vectors requires 'index_name' or 'index_arn'"
            ));
        }
        if index_name.is_some() && bucket_name.is_none() {
            return Err(anyhow::anyhow!(
                "s3vector_delete_vectors requires a vector bucket when using 'index_name'"
            ));
        }

        let keys = resolve_string_array(
            config,
            "keys",
            Some("keys_source_key"),
            &ctx,
            "s3vector_delete_vectors",
            "keys",
        )?;

        let mut request = build_s3vector_client(config, &ctx)
            .await?
            .delete_vectors()
            .set_keys(Some(keys.clone()));
        if let Some(bucket_name) = bucket_name {
            request = request.vector_bucket_name(bucket_name);
        }
        if let Some(index_name) = index_name {
            request = request.index_name(index_name);
        }
        if let Some(index_arn) = index_arn {
            request = request.index_arn(index_arn);
        }

        let _response = request.send().await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_deleted_count", output_key),
            serde_json::json!(keys.len()),
        );
        output.insert(
            format!("{}_deleted_keys", output_key),
            serde_json::Value::Array(keys.into_iter().map(serde_json::Value::String).collect()),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}
