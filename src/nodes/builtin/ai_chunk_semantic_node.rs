use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::ai_embed_node::{acquire_oauth_token, embed_ollama, embed_openai, resolve_param};

// =============================================================================
// Sentence Splitting
// =============================================================================

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();

    let mut i = 0;
    while i < len {
        if (bytes[i] == b'.' || bytes[i] == b'!' || bytes[i] == b'?')
            && (i + 1 >= len || bytes[i + 1].is_ascii_whitespace())
        {
            // End of sentence at delimiter
            let end = i + 1;
            let sentence = &text[start..end];
            if !sentence.trim().is_empty() {
                sentences.push(sentence.to_string());
            }
            // Skip trailing whitespace
            i = end;
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }

    // Remaining text
    if start < len {
        let remainder = &text[start..];
        if !remainder.trim().is_empty() {
            sentences.push(remainder.to_string());
        }
    }

    sentences
}

// =============================================================================
// Matrix Operations (ported from cognigraph-chunker savgol.rs)
// =============================================================================

fn matrix_multiply(a: &[f64], b: &[f64], m: usize, n: usize, p: usize) -> Vec<f64> {
    let mut c = vec![0.0; m * p];
    for i in 0..m {
        for j in 0..p {
            let mut sum = 0.0;
            for k in 0..n {
                sum += a[i * n + k] * b[k * p + j];
            }
            c[i * p + j] = sum;
        }
    }
    c
}

fn matrix_transpose(a: &[f64], m: usize, n: usize) -> Vec<f64> {
    let mut at = vec![0.0; n * m];
    for i in 0..m {
        for j in 0..n {
            at[j * m + i] = a[i * n + j];
        }
    }
    at
}

fn matrix_inverse(a: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut a_inv = vec![0.0; n * n];
    for i in 0..n {
        a_inv[i * n + i] = 1.0;
    }

    let mut work = a.to_vec();

    for i in 0..n {
        let mut max_row = i;
        let mut max_val = work[i * n + i].abs();
        for k in (i + 1)..n {
            let val = work[k * n + i].abs();
            if val > max_val {
                max_val = val;
                max_row = k;
            }
        }

        if max_row != i {
            for j in 0..n {
                work.swap(i * n + j, max_row * n + j);
                a_inv.swap(i * n + j, max_row * n + j);
            }
        }

        let pivot = work[i * n + i];
        if pivot.abs() < 1e-10 {
            return None;
        }

        for j in 0..n {
            work[i * n + j] /= pivot;
            a_inv[i * n + j] /= pivot;
        }

        for k in 0..n {
            if k != i {
                let factor = work[k * n + i];
                for j in 0..n {
                    work[k * n + j] -= factor * work[i * n + j];
                    a_inv[k * n + j] -= factor * a_inv[i * n + j];
                }
            }
        }
    }

    Some(a_inv)
}

// =============================================================================
// Savitzky-Golay Filter
// =============================================================================

fn compute_savgol_coeffs(window_size: usize, poly_order: usize, deriv: usize) -> Option<Vec<f64>> {
    let half_window = (window_size - 1) / 2;
    let poly_cols = poly_order + 1;

    let mut a = vec![0.0; window_size * poly_cols];
    for i in 0..window_size {
        let x = i as f64 - half_window as f64;
        for j in 0..poly_cols {
            a[i * poly_cols + j] = x.powi(j as i32);
        }
    }

    let at = matrix_transpose(&a, window_size, poly_cols);
    let ata = matrix_multiply(&at, &a, poly_cols, window_size, poly_cols);
    let ata_inv = matrix_inverse(&ata, poly_cols)?;

    let factorial: f64 = (1..=deriv).map(|i| i as f64).product::<f64>().max(1.0);

    let mut coeffs = vec![0.0; window_size];
    for i in 0..window_size {
        if deriv < poly_cols {
            let mut sum = 0.0;
            for k in 0..poly_cols {
                sum += ata_inv[deriv * poly_cols + k] * a[i * poly_cols + k];
            }
            coeffs[i] = factorial * sum;
        }
    }

    Some(coeffs)
}

fn apply_convolution(data: &[f64], kernel: &[f64]) -> Vec<f64> {
    let n = data.len();
    let kernel_size = kernel.len();
    let half = kernel_size / 2;
    let mut output = vec![0.0; n];

    for (i, out) in output.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (j, &k) in kernel.iter().enumerate() {
            let mut idx = i as isize - half as isize + j as isize;
            if idx < 0 {
                idx = -idx;
            } else if idx >= n as isize {
                idx = 2 * n as isize - idx - 2;
            }
            idx = idx.clamp(0, n as isize - 1);
            sum += data[idx as usize] * k;
        }
        *out = sum;
    }

    output
}

fn savgol_filter(
    data: &[f64],
    window_length: usize,
    poly_order: usize,
    deriv: usize,
) -> Option<Vec<f64>> {
    if window_length.is_multiple_of(2) || window_length <= poly_order || data.is_empty() {
        return None;
    }

    let coeffs = compute_savgol_coeffs(window_length, poly_order, deriv)?;
    Some(apply_convolution(data, &coeffs))
}

// =============================================================================
// Windowed Cross-Similarity
// =============================================================================

fn windowed_cross_similarity(
    embeddings: &[f64],
    n: usize,
    d: usize,
    window_size: usize,
) -> Option<Vec<f64>> {
    if window_size.is_multiple_of(2) || window_size < 3 || n < 2 || d == 0 {
        return None;
    }

    let half_window = window_size / 2;
    let mut result = vec![0.0; n - 1];

    for (i, slot) in result.iter_mut().enumerate() {
        let start = i.saturating_sub(half_window);
        let end = (i + half_window + 2).min(n);

        let mut total_sim = 0.0;
        let mut count = 0;

        for j in start..(end - 1) {
            let emb1_start = j * d;
            let emb2_start = (j + 1) * d;

            let mut dot = 0.0;
            let mut norm1 = 0.0;
            let mut norm2 = 0.0;

            for k in 0..d {
                let v1 = embeddings[emb1_start + k];
                let v2 = embeddings[emb2_start + k];
                dot += v1 * v2;
                norm1 += v1 * v1;
                norm2 += v2 * v2;
            }

            if norm1 > 0.0 && norm2 > 0.0 {
                total_sim += dot / (norm1.sqrt() * norm2.sqrt());
                count += 1;
            }
        }

        *slot = if count > 0 {
            1.0 - (total_sim / count as f64)
        } else {
            0.0
        };
    }

    Some(result)
}

// =============================================================================
// Local Minima Detection
// =============================================================================

fn find_local_minima_interpolated(
    data: &[f64],
    window_size: usize,
    poly_order: usize,
    tolerance: f64,
) -> Option<(Vec<usize>, Vec<f64>)> {
    if data.is_empty() {
        return Some((vec![], vec![]));
    }

    let first_deriv = savgol_filter(data, window_size, poly_order, 1)?;
    let second_deriv = savgol_filter(data, window_size, poly_order, 2)?;

    let mut indices = Vec::new();
    let mut values = Vec::new();

    for i in 0..data.len() {
        if first_deriv[i].abs() < tolerance && second_deriv[i] > 0.0 {
            indices.push(i);
            values.push(data[i]);
        }
    }

    Some((indices, values))
}

// =============================================================================
// Split Index Filtering
// =============================================================================

fn percentile(data: &[f64], p: f64) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let idx = p * (sorted.len() - 1) as f64;
    let lower = idx.floor() as usize;
    let upper = (lower + 1).min(sorted.len() - 1);
    let weight = idx - lower as f64;

    sorted[lower] * (1.0 - weight) + sorted[upper] * weight
}

fn filter_split_indices(
    indices: &[usize],
    values: &[f64],
    threshold: f64,
    min_distance: usize,
) -> (Vec<usize>, Vec<f64>) {
    let threshold = if threshold.is_nan() {
        0.0
    } else {
        threshold.clamp(0.0, 1.0)
    };

    if indices.is_empty() || values.is_empty() {
        return (vec![], vec![]);
    }

    let threshold_val = percentile(values, threshold);

    let mut result_indices = Vec::new();
    let mut result_values = Vec::new();
    let mut last_idx: Option<usize> = None;

    for (&idx, &val) in indices.iter().zip(values.iter()) {
        let distance_ok = match last_idx {
            Some(last) => idx >= last + min_distance,
            None => true,
        };

        if val <= threshold_val && distance_ok {
            result_indices.push(idx);
            result_values.push(val);
            last_idx = Some(idx);
        }
    }

    (result_indices, result_values)
}

// =============================================================================
// Helpers
// =============================================================================

fn clamp_odd_window(window: usize, data_len: usize) -> usize {
    let w = window.min(data_len);
    let w = if w.is_multiple_of(2) {
        w.saturating_sub(1)
    } else {
        w
    };
    w.max(3).min(data_len)
}

fn group_sentences_at_boundaries(sentences: &[String], split_indices: &[usize]) -> Vec<String> {
    if sentences.is_empty() {
        return vec![];
    }

    let mut chunks = Vec::new();
    let mut chunk_start = 0;

    for &split_idx in split_indices {
        let chunk_end = split_idx + 1;
        if chunk_end > chunk_start && chunk_end <= sentences.len() {
            let chunk_text: String = sentences[chunk_start..chunk_end].join(" ");
            chunks.push(chunk_text);
            chunk_start = chunk_end;
        }
    }

    // Remaining sentences form the last chunk
    if chunk_start < sentences.len() {
        let chunk_text: String = sentences[chunk_start..].join(" ");
        chunks.push(chunk_text);
    }

    chunks
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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("ai_chunk_semantic requires 'source_key' parameter"))?;
        let source_key = crate::lua::interpolate::interpolate_ctx(source_key, &ctx);

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
                    resolve_param(config, "api_key", "OPENAI_API_KEY", &ctx).ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk_semantic (openai) requires 'api_key' or OPENAI_API_KEY env var"
                        )
                    })?;
                let base_url = resolve_param(config, "base_url", "OPENAI_BASE_URL", &ctx)
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
                let model = config
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text-embedding-3-small");

                embed_openai(&client, &base_url, &api_key, model, &sentences).await?
            }
            "ollama" => {
                let host = resolve_param(config, "ollama_host", "OLLAMA_HOST", &ctx)
                    .unwrap_or_else(|| "http://localhost:11434".to_string());
                let model = config
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("nomic-embed-text");

                embed_ollama(&client, &host, model, &sentences).await?
            }
            "oauth" => {
                let token_url = resolve_param(config, "token_url", "OAUTH_TOKEN_URL", &ctx)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk_semantic (oauth) requires 'token_url' or OAUTH_TOKEN_URL env var"
                        )
                    })?;
                let client_id = resolve_param(config, "client_id", "OAUTH_CLIENT_ID", &ctx)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk_semantic (oauth) requires 'client_id' or OAUTH_CLIENT_ID env var"
                        )
                    })?;
                let client_secret =
                    resolve_param(config, "client_secret", "OAUTH_CLIENT_SECRET", &ctx)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "ai_chunk_semantic (oauth) requires 'client_secret' or OAUTH_CLIENT_SECRET env var"
                            )
                        })?;
                let scope = resolve_param(config, "scope", "OAUTH_SCOPE", &ctx);
                let base_url = resolve_param(config, "base_url", "OAUTH_BASE_URL", &ctx)
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
