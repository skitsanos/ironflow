use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct AiChunkNode;

/// Find last occurrence of any delimiter byte in window, using simple iteration.
fn find_last_delim(window: &[u8], delimiters: &[u8]) -> Option<usize> {
    if delimiters.is_empty() {
        return None;
    }
    (0..window.len())
        .rev()
        .find(|&i| delimiters.contains(&window[i]))
}

/// Find first occurrence of any delimiter byte in window, using simple iteration.
fn find_first_delim(window: &[u8], delimiters: &[u8]) -> Option<usize> {
    for (i, &b) in window.iter().enumerate() {
        if delimiters.contains(&b) {
            return Some(i);
        }
    }
    None
}

/// Fixed-size chunking: walk text in `size`-byte windows, splitting at delimiter boundaries.
fn chunk_fixed(text: &str, size: usize, delimiters: &[u8], prefix: bool) -> Vec<String> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return vec![];
    }

    let size = size.max(1);
    let mut chunks = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        let remaining = bytes.len() - pos;

        // Last chunk — return remainder
        if remaining <= size {
            chunks.push(String::from_utf8_lossy(&bytes[pos..]).into_owned());
            break;
        }

        let end = (pos + size).min(bytes.len());
        let window = &bytes[pos..end];

        // Search backward for a delimiter
        if let Some(rel_pos) = find_last_delim(window, delimiters) {
            let abs_pos = pos + rel_pos;
            if abs_pos == pos {
                // Delimiter at very start of window — hard split
                let chunk = &bytes[pos..end];
                chunks.push(String::from_utf8_lossy(chunk).into_owned());
                pos = end;
            } else if prefix {
                // Delimiter goes to start of next chunk
                let split_at = abs_pos;
                chunks.push(String::from_utf8_lossy(&bytes[pos..split_at]).into_owned());
                pos = split_at;
            } else {
                // Delimiter stays with current chunk (suffix mode)
                let split_at = abs_pos + 1;
                chunks.push(String::from_utf8_lossy(&bytes[pos..split_at]).into_owned());
                pos = split_at;
            }
        } else {
            // No delimiter found — hard split at size
            chunks.push(String::from_utf8_lossy(&bytes[pos..end]).into_owned());
            pos = end;
        }
    }

    chunks
}

/// Delimiter-based splitting: split at every delimiter occurrence.
fn chunk_split(text: &str, delimiters: &[u8], min_chars: usize) -> Vec<String> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return vec![];
    }
    if delimiters.is_empty() {
        return vec![text.to_string()];
    }

    // Split at each delimiter, attaching delimiter to previous segment
    let mut segments: Vec<(usize, usize)> = Vec::new();
    let mut seg_start = 0;
    let mut pos = 0;

    while pos < bytes.len() {
        match find_first_delim(&bytes[pos..], delimiters) {
            Some(rel_pos) => {
                let abs_pos = pos + rel_pos;
                let seg_end = abs_pos + 1; // include delimiter with previous
                if seg_start < seg_end {
                    segments.push((seg_start, seg_end));
                }
                seg_start = seg_end;
                pos = seg_end;
            }
            None => {
                // No more delimiters — remainder is final segment
                if seg_start < bytes.len() {
                    segments.push((seg_start, bytes.len()));
                }
                break;
            }
        }
    }

    // Handle trailing content if loop ended via delimiter at very end
    if seg_start < bytes.len()
        && (segments.is_empty() || segments.last().is_none_or(|&(_, e)| e < bytes.len()))
    {
        segments.push((seg_start, bytes.len()));
    }

    // Merge short segments into previous
    if min_chars > 0 && segments.len() > 1 {
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (s, e) in segments {
            let len = e - s;
            if let Some(last) = merged.last_mut() {
                let last_len = last.1 - last.0;
                if last_len < min_chars || len < min_chars {
                    last.1 = e; // extend previous
                    continue;
                }
            }
            merged.push((s, e));
        }
        segments = merged;
    }

    segments
        .into_iter()
        .map(|(s, e)| String::from_utf8_lossy(&bytes[s..e]).into_owned())
        .collect()
}

#[async_trait]
impl Node for AiChunkNode {
    fn node_type(&self) -> &str {
        "ai_chunk"
    }

    fn description(&self) -> &str {
        "Split text into chunks using fixed-size or delimiter strategies"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let mode = config
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("fixed");

        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("ai_chunk requires 'source_key' parameter"))?;
        let source_key = interpolate_ctx(source_key, &ctx);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("chunks")
            .to_string();

        // Get source text from context
        let text = ctx
            .get(&source_key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "ai_chunk: source_key '{}' not found or not a string in context",
                    source_key
                )
            })?;

        let chunks = match mode {
            "fixed" => {
                let size = config.get("size").and_then(|v| v.as_u64()).unwrap_or(4096) as usize;

                let delimiters_str = config
                    .get("delimiters")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let delimiters = delimiters_str.as_bytes();

                let prefix = config
                    .get("prefix")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                chunk_fixed(&text, size, delimiters, prefix)
            }
            "split" => {
                let delimiters_str = config
                    .get("delimiters")
                    .and_then(|v| v.as_str())
                    .unwrap_or("\n.?");
                let delimiters = delimiters_str.as_bytes();

                let min_chars = config
                    .get("min_chars")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;

                chunk_split(&text, delimiters, min_chars)
            }
            other => anyhow::bail!(
                "ai_chunk: unsupported mode '{}' (use 'fixed' or 'split')",
                other
            ),
        };

        let count = chunks.len();
        let chunks_json: Vec<serde_json::Value> =
            chunks.into_iter().map(serde_json::Value::String).collect();

        let mut output = NodeOutput::new();
        output.insert(output_key.clone(), serde_json::Value::Array(chunks_json));
        output.insert(format!("{}_count", output_key), serde_json::json!(count));
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );

        Ok(output)
    }
}
