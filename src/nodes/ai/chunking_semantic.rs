use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::chunking_semantic_engine::{
    clamp_odd_window, filter_split_indices, find_local_minima_interpolated,
    group_sentences_at_boundaries, savgol_filter, split_sentences, windowed_cross_similarity,
};
use super::embeddings::{acquire_oauth_token, embed_ollama, embed_openai, resolve_param};

// =============================================================================
// Node Implementation
// =============================================================================

pub struct AiChunkSemanticNode;

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
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("ai_chunk_semantic requires 'source_key' parameter"))?;
        let source_key = crate::lua::interpolate::interpolate_ctx(source_key, ctx);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("semantic");

        let provider = config
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("openai");

        let timeout_s = config
            .get("timeout")
            .and_then(|v| v.as_f64())
            .unwrap_or(120.0);

        let sim_window = config
            .get("sim_window")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(3);
        let sim_window = if sim_window < 3 {
            3
        } else if sim_window.is_multiple_of(2) {
            sim_window + 1
        } else {
            sim_window
        };

        let sg_window = config
            .get("sg_window")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(11);
        let sg_window = if sg_window.is_multiple_of(2) {
            sg_window + 1
        } else {
            sg_window
        };

        let poly_order = config
            .get("poly_order")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(3);

        let threshold = config
            .get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);

        let min_distance = config
            .get("min_distance")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(2);

        // Get source text from context
        let text = ctx
            .get(&source_key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "ai_chunk_semantic: source_key '{}' not found or not a string in context",
                    source_key
                )
            })?;

        // Edge case: empty text
        if text.trim().is_empty() {
            let mut output = NodeOutput::new();
            output.insert(output_key.to_string(), serde_json::json!([]));
            output.insert(format!("{}_count", output_key), serde_json::json!(0));
            output.insert(
                format!("{}_success", output_key),
                serde_json::Value::Bool(true),
            );
            return Ok(output);
        }

        // Step 1: Split text into sentences
        let sentences = split_sentences(&text);

        // Edge case: single sentence or too few for windowing
        if sentences.len() <= 1 {
            let chunks = vec![text.clone()];
            let mut output = NodeOutput::new();
            output.insert(output_key.to_string(), serde_json::json!(chunks));
            output.insert(format!("{}_count", output_key), serde_json::json!(1));
            output.insert(
                format!("{}_success", output_key),
                serde_json::Value::Bool(true),
            );
            return Ok(output);
        }

        // Step 2: Embed all sentences
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs_f64(timeout_s))
            .build()?;

        let embeddings = match provider {
            "openai" => {
                let api_key =
                    resolve_param(config, "api_key", "OPENAI_API_KEY", ctx).ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk_semantic (openai) requires 'api_key' or OPENAI_API_KEY env var"
                        )
                    })?;
                let base_url = resolve_param(config, "base_url", "OPENAI_BASE_URL", ctx)
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
                let model = config
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text-embedding-3-small");

                embed_openai(&client, &base_url, &api_key, model, &sentences).await?
            }
            "ollama" => {
                let host = resolve_param(config, "ollama_host", "OLLAMA_HOST", ctx)
                    .unwrap_or_else(|| "http://localhost:11434".to_string());
                let model = config
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("nomic-embed-text");

                embed_ollama(&client, &host, model, &sentences).await?
            }
            "oauth" => {
                let token_url = resolve_param(config, "token_url", "OAUTH_TOKEN_URL", ctx)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk_semantic (oauth) requires 'token_url' or OAUTH_TOKEN_URL env var"
                        )
                    })?;
                let client_id = resolve_param(config, "client_id", "OAUTH_CLIENT_ID", ctx)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk_semantic (oauth) requires 'client_id' or OAUTH_CLIENT_ID env var"
                        )
                    })?;
                let client_secret =
                    resolve_param(config, "client_secret", "OAUTH_CLIENT_SECRET", ctx)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "ai_chunk_semantic (oauth) requires 'client_secret' or OAUTH_CLIENT_SECRET env var"
                            )
                        })?;
                let scope = resolve_param(config, "scope", "OAUTH_SCOPE", ctx);
                let base_url = resolve_param(config, "base_url", "OAUTH_BASE_URL", ctx)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk_semantic (oauth) requires 'base_url' or OAUTH_BASE_URL env var"
                        )
                    })?;
                let model = config
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("openai-text-embedding-3-small");

                let token = acquire_oauth_token(
                    &client,
                    &token_url,
                    &client_id,
                    &client_secret,
                    scope.as_deref(),
                )
                .await?;

                embed_openai(&client, &base_url, &token, model, &sentences).await?
            }
            other => anyhow::bail!("ai_chunk_semantic: unsupported provider '{}'", other),
        };

        if embeddings.len() != sentences.len() {
            anyhow::bail!(
                "ai_chunk_semantic: provider returned {} embeddings for {} sentences",
                embeddings.len(),
                sentences.len()
            );
        }

        let n = sentences.len();
        let dim = embeddings.first().map(|v| v.len()).unwrap_or(0);

        if dim == 0 {
            anyhow::bail!("ai_chunk_semantic: embedding dimension is 0");
        }

        // Flatten embeddings for windowed_cross_similarity
        let flat_embeddings: Vec<f64> = embeddings.iter().flat_map(|e| e.iter().copied()).collect();

        // Step 3: Compute windowed cross-similarity (distance curve)
        let similarities = match windowed_cross_similarity(&flat_embeddings, n, dim, sim_window) {
            Some(s) => s,
            None => {
                // Fallback: return entire text as one chunk
                let chunks = vec![text.clone()];
                let mut output = NodeOutput::new();
                output.insert(output_key.to_string(), serde_json::json!(chunks));
                output.insert(format!("{}_count", output_key), serde_json::json!(1));
                output.insert(
                    format!("{}_success", output_key),
                    serde_json::Value::Bool(true),
                );
                return Ok(output);
            }
        };

        // Step 4: Smooth with Savitzky-Golay filter
        let effective_sg = clamp_odd_window(sg_window, similarities.len());
        let effective_sg = if effective_sg <= poly_order {
            0
        } else {
            effective_sg
        };

        let smoothed = if effective_sg >= 3 {
            savgol_filter(&similarities, effective_sg, poly_order, 0)
                .unwrap_or_else(|| similarities.clone())
        } else {
            similarities.clone()
        };

        // Step 5: Find local minima
        let minima_window = clamp_odd_window(effective_sg.max(5), smoothed.len());

        let (minima_indices, minima_values) = if minima_window >= 3 && minima_window > poly_order {
            find_local_minima_interpolated(&smoothed, minima_window, poly_order, 0.1)
                .unwrap_or_else(|| (vec![], vec![]))
        } else {
            (vec![], vec![])
        };

        // Step 6: Filter split points
        let (split_indices, _) =
            filter_split_indices(&minima_indices, &minima_values, threshold, min_distance);

        // Step 7: Group sentences at boundaries
        let chunks = group_sentences_at_boundaries(&sentences, &split_indices);
        let count = chunks.len();

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::json!(chunks));
        output.insert(format!("{}_count", output_key), serde_json::json!(count));
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );

        Ok(output)
    }
}
