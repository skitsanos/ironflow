mod config;
mod oauth;
mod provider;
mod response;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::duration::positive_duration;
use crate::util::node_config::config_f64_or;

pub(super) use config::resolve_param;
pub(super) use oauth::acquire_oauth_token;
pub(super) use provider::{embed_ollama, embed_openai};

pub struct AiEmbedNode;

fn resolve_texts(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(text) => vec![text.clone()],
        serde_json::Value::Array(items) => items
            .iter()
            .map(|item| match item {
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .collect(),
        other => vec![other.to_string()],
    }
}

fn build_output(output_key: &str, embeddings: Vec<Vec<f64>>, model: &str) -> NodeOutput {
    let count = embeddings.len();
    let dimension = embeddings.first().map(Vec::len).unwrap_or(0);
    let embeddings = embeddings
        .into_iter()
        .map(|embedding| serde_json::json!(embedding))
        .collect();

    let mut output = NodeOutput::new();
    output.insert(
        format!("{}_embeddings", output_key),
        serde_json::Value::Array(embeddings),
    );
    output.insert(format!("{}_count", output_key), serde_json::json!(count));
    output.insert(
        format!("{}_dimension", output_key),
        serde_json::json!(dimension),
    );
    output.insert(format!("{}_model", output_key), serde_json::json!(model));
    output.insert(
        format!("{}_success", output_key),
        serde_json::Value::Bool(true),
    );
    output
}

#[async_trait]
impl Node for AiEmbedNode {
    fn node_type(&self) -> &str {
        "ai_embed"
    }

    fn description(&self) -> &str {
        "Generate text embeddings via OpenAI, Ollama, or OAuth providers"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let input_key = config
            .get("input_key")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("ai_embed requires 'input_key' parameter"))?;
        let input_key = crate::lua::interpolate::interpolate_ctx(input_key, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|value| value.as_str())
            .unwrap_or("embed");
        let timeout_s = config_f64_or(config, "timeout", ctx, 120.0)?;

        let input_value = ctx.get(&input_key).ok_or_else(|| {
            anyhow::anyhow!("ai_embed: input_key '{}' not found in context", input_key)
        })?;
        let texts = resolve_texts(input_value);
        if texts.is_empty() {
            return Ok(build_output(output_key, Vec::new(), ""));
        }

        let client = reqwest::Client::builder()
            .timeout(positive_duration(timeout_s, "ai_embed timeout")?)
            .build()?;
        let (embeddings, model) = provider::embed_for_config(&client, config, ctx, &texts).await?;

        Ok(build_output(output_key, embeddings, &model))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolve_texts_preserves_strings_and_serializes_other_values() {
        assert_eq!(
            resolve_texts(&json!(["first", 2, true])),
            vec!["first", "2", "true"]
        );
    }

    #[test]
    fn empty_output_preserves_public_shape() {
        let output = build_output("embed", Vec::new(), "");
        assert_eq!(output.get("embed_embeddings"), Some(&json!([])));
        assert_eq!(output.get("embed_count"), Some(&json!(0)));
        assert_eq!(output.get("embed_dimension"), Some(&json!(0)));
        assert_eq!(output.get("embed_model"), Some(&json!("")));
        assert_eq!(output.get("embed_success"), Some(&json!(true)));
    }
}
