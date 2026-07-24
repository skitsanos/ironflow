use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3vectors::operation::put_vectors::builders::PutVectorsInputBuilder;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::resolve_output_key;
use super::target::{IndexTarget, TargetPolicy, resolve_index_target};
use super::vectors::resolve_vectors_data;

struct PreparedPutVectors {
    request: PutVectorsInputBuilder,
    vector_keys: Vec<String>,
}

fn prepare_put_vectors_input(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<PreparedPutVectors> {
    let target = resolve_index_target(
        config,
        ctx,
        "s3vector_put_vectors",
        TargetPolicy::AllowEnvironment,
    )?;
    let vectors = resolve_vectors_data(config, ctx, "s3vector_put_vectors")?;
    let vector_keys = vectors
        .iter()
        .map(|value| value.key().to_string())
        .collect();
    let request = PutVectorsInputBuilder::default().set_vectors(Some(vectors));
    let request = match target {
        IndexTarget::Names {
            bucket_name,
            index_name,
        } => request
            .vector_bucket_name(bucket_name)
            .index_name(index_name),
        IndexTarget::Arn(arn) => request.index_arn(arn),
    };
    Ok(PreparedPutVectors {
        request,
        vector_keys,
    })
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

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let prepared = prepare_put_vectors_input(config, ctx)?;
        let client = build_s3vector_client(config, ctx).await?;
        let _response = prepared.request.send_with(&client).await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_vector_count", output_key),
            serde_json::json!(prepared.vector_keys.len()),
        );
        output.insert(
            format!("{}_vector_keys", output_key),
            serde_json::Value::Array(
                prepared
                    .vector_keys
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

#[cfg(test)]
mod tests;
