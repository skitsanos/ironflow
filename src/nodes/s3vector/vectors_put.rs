use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::client::build_s3vector_client;
use super::config::{resolve_bucket_id, resolve_index_id, resolve_output_key};
use super::vectors::resolve_vectors_data;

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
        let (bucket_name, _bucket_arn) = resolve_bucket_id(config, ctx, "s3vector_put_vectors")?;
        let (index_name, index_arn) = resolve_index_id(config, ctx, "s3vector_put_vectors")?;
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

        let vectors = resolve_vectors_data(config, ctx, "s3vector_put_vectors")?;
        let vector_keys: Vec<String> = vectors
            .iter()
            .map(|value| value.key().to_string())
            .collect();

        let mut request = build_s3vector_client(config, ctx)
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
