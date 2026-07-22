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
use crate::util::node_config::{config_f64, config_u64};

// =============================================================================
// Parameters
// =============================================================================

/// Tuning parameters for the semantic splitter, with defaults and range clamping applied.
///
/// Read through `config_f64` / `config_u64` so a parameter written as `"${ctx.key}"`
/// resolves like any string parameter would.
struct SemanticChunkParams {
    timeout_s: f64,
    sim_window: usize,
    sg_window: usize,
    poly_order: usize,
    threshold: f64,
    min_distance: usize,
}

impl SemanticChunkParams {
    fn from_config(config: &serde_json::Value, ctx: &Context) -> Self {
        let sim_window = config_u64(config, "sim_window", ctx)
            .map(|v| v as usize)
            .unwrap_or(3);
        let sim_window = if sim_window < 3 {
            3
        } else if sim_window.is_multiple_of(2) {
            sim_window + 1
        } else {
            sim_window
        };

        let sg_window = config_u64(config, "sg_window", ctx)
            .map(|v| v as usize)
            .unwrap_or(11);
        let sg_window = if sg_window.is_multiple_of(2) {
            sg_window + 1
        } else {
            sg_window
        };

        Self {
            timeout_s: config_f64(config, "timeout", ctx).unwrap_or(120.0),
            sim_window,
            sg_window,
            poly_order: config_u64(config, "poly_order", ctx)
                .map(|v| v as usize)
                .unwrap_or(3),
            threshold: config_f64(config, "threshold", ctx)
                .unwrap_or(0.5)
                .clamp(0.0, 1.0),
            min_distance: config_u64(config, "min_distance", ctx)
                .map(|v| v as usize)
                .unwrap_or(2),
        }
    }
}

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

        let SemanticChunkParams {
            timeout_s,
            sim_window,
            sg_window,
            poly_order,
            threshold,
            min_distance,
        } = SemanticChunkParams::from_config(config, ctx);

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx_with(key: &str, value: serde_json::Value) -> Context {
        let mut ctx = Context::new();
        ctx.insert(key.to_string(), value);
        ctx
    }

    #[test]
    fn threshold_resolves_from_interpolated_context() {
        let ctx = ctx_with("threshold", json!(0.3));
        let config = json!({ "threshold": "${ctx.threshold}" });
        assert_eq!(
            SemanticChunkParams::from_config(&config, &ctx).threshold,
            0.3
        );
    }

    #[test]
    fn threshold_defaults_when_absent() {
        let params = SemanticChunkParams::from_config(&json!({}), &Context::new());
        assert_eq!(params.threshold, 0.5);
    }

    #[test]
    fn threshold_is_clamped_to_unit_range() {
        let params =
            SemanticChunkParams::from_config(&json!({ "threshold": 4.2 }), &Context::new());
        assert_eq!(params.threshold, 1.0);
    }

    #[test]
    fn sim_window_is_forced_odd_and_at_least_three() {
        let ctx = Context::new();
        assert_eq!(
            SemanticChunkParams::from_config(&json!({ "sim_window": 1 }), &ctx).sim_window,
            3
        );
        assert_eq!(
            SemanticChunkParams::from_config(&json!({ "sim_window": 6 }), &ctx).sim_window,
            7
        );
    }

    #[test]
    fn sg_window_is_forced_odd() {
        let params = SemanticChunkParams::from_config(&json!({ "sg_window": 10 }), &Context::new());
        assert_eq!(params.sg_window, 11);
    }

    #[test]
    fn windows_resolve_from_interpolated_context() {
        let ctx = ctx_with("window", json!(15));
        let config = json!({ "sg_window": "${ctx.window}", "min_distance": "${ctx.window}" });
        let params = SemanticChunkParams::from_config(&config, &ctx);
        assert_eq!(params.sg_window, 15);
        assert_eq!(params.min_distance, 15);
    }

    #[test]
    fn timeout_resolves_from_interpolated_context() {
        let ctx = ctx_with("timeout", json!(45));
        let config = json!({ "timeout": "${ctx.timeout}" });
        assert_eq!(
            SemanticChunkParams::from_config(&config, &ctx).timeout_s,
            45.0
        );
    }
}
