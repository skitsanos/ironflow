use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3vectors::operation::delete_vectors::builders::DeleteVectorsInputBuilder;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::resolve_output_key;
use super::parameters::resolve_string_array;
use super::target::{IndexTarget, TargetPolicy, resolve_index_target};

struct PreparedDeleteVectors {
    request: DeleteVectorsInputBuilder,
    keys: Vec<String>,
}

fn prepare_delete_vectors_input(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<PreparedDeleteVectors> {
    let target = resolve_index_target(
        config,
        ctx,
        "s3vector_delete_vectors",
        TargetPolicy::ExplicitOnly,
    )?;
    let keys = resolve_string_array(
        config,
        "keys",
        Some("keys_source_key"),
        ctx,
        "s3vector_delete_vectors",
        "keys",
    )?;
    let request = DeleteVectorsInputBuilder::default().set_keys(Some(keys.clone()));
    let request = match target {
        IndexTarget::Names {
            bucket_name,
            index_name,
        } => request
            .vector_bucket_name(bucket_name)
            .index_name(index_name),
        IndexTarget::Arn(arn) => request.index_arn(arn),
    };
    Ok(PreparedDeleteVectors { request, keys })
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

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = resolve_output_key(config);
        let prepared = prepare_delete_vectors_input(config, ctx)?;
        let client = build_s3vector_client(config, ctx).await?;
        let _response = prepared.request.send_with(&client).await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_deleted_count", output_key),
            serde_json::json!(prepared.keys.len()),
        );
        output.insert(
            format!("{}_deleted_keys", output_key),
            serde_json::Value::Array(
                prepared
                    .keys
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
