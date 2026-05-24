use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::{resolve_bucket_id, resolve_index_id, resolve_output_key};
use super::parameters::resolve_string_array;

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
        let (bucket_name, _bucket_arn) = resolve_bucket_id(config, ctx, "s3vector_delete_vectors")?;
        let (index_name, index_arn) = resolve_index_id(config, ctx, "s3vector_delete_vectors")?;
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
            ctx,
            "s3vector_delete_vectors",
            "keys",
        )?;

        let mut request = build_s3vector_client(config, ctx)
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
