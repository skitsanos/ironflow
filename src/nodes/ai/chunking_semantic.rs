mod config;
mod pipeline;
mod provider;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::duration::positive_duration;

use self::config::SemanticChunkParams;
use super::chunking_semantic_engine::split_sentences;

pub struct AiChunkSemanticNode;

fn build_output(output_key: &str, chunks: Vec<String>) -> NodeOutput {
    let count = chunks.len();
    let mut output = NodeOutput::new();
    output.insert(output_key.to_string(), serde_json::json!(chunks));
    output.insert(format!("{}_count", output_key), serde_json::json!(count));
    output.insert(
        format!("{}_success", output_key),
        serde_json::Value::Bool(true),
    );
    output
}

#[async_trait]
impl Node for AiChunkSemanticNode {
    fn node_type(&self) -> &str {
        "ai_chunk_semantic"
    }

    fn description(&self) -> &str {
        "Split text into semantic chunks using embedding similarity"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("ai_chunk_semantic requires 'source_key' parameter"))?;
        let source_key = crate::lua::interpolate::interpolate_ctx(source_key, ctx);
        let output_key = config
            .get("output_key")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("semantic");
        let params = SemanticChunkParams::from_config(config, ctx)?;
        let text = ctx
            .get(&source_key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "ai_chunk_semantic: source_key '{}' not found or not a string in context",
                    source_key
                )
            })?;

        if text.trim().is_empty() {
            return Ok(build_output(output_key, Vec::new()));
        }
        let sentences = split_sentences(&text);
        if sentences.len() <= 1 {
            return Ok(build_output(output_key, vec![text]));
        }

        let client = reqwest::Client::builder()
            .timeout(positive_duration(
                params.timeout_s,
                "ai_chunk_semantic timeout",
            )?)
            .build()?;
        let embeddings = provider::embed_sentences(&client, config, ctx, &sentences).await?;
        let chunks =
            pipeline::build_chunks(&sentences, &embeddings, &params)?.unwrap_or_else(|| vec![text]);

        Ok(build_output(output_key, chunks))
    }
}
