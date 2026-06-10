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

/// Group ordered subtitle cues into size-bounded segments that retain the
/// min-start / max-end timestamps of the cues in each group. A single cue is
/// never split: a cue whose text alone exceeds `size` becomes its own segment.
fn chunk_cues(cues: &[serde_json::Value], size: usize) -> Result<Vec<serde_json::Value>> {
    let mut segments = Vec::new();
    let mut group: Vec<&serde_json::Value> = Vec::new();
    let mut group_chars = 0usize;

    for (i, cue) in cues.iter().enumerate() {
        let obj = cue
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("ai_chunk: cue at index {} is not an object", i))?;
        let text = obj.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
            anyhow::anyhow!(
                "ai_chunk: cue at index {} is missing a string 'text' field",
                i
            )
        })?;
        if obj.get("start_ms").and_then(|v| v.as_u64()).is_none() {
            anyhow::bail!("ai_chunk: cue at index {} is missing numeric 'start_ms'", i);
        }
        if obj.get("end_ms").and_then(|v| v.as_u64()).is_none() {
            anyhow::bail!("ai_chunk: cue at index {} is missing numeric 'end_ms'", i);
        }

        let cue_chars = text.chars().count();
        let added = if group.is_empty() {
            cue_chars
        } else {
            cue_chars + 1
        };

        if !group.is_empty() && group_chars + added > size {
            segments.push(build_cue_segment(&group));
            group.clear();
            group_chars = 0;
        }

        group_chars += if group.is_empty() {
            cue_chars
        } else {
            cue_chars + 1
        };
        group.push(cue);
    }

    if !group.is_empty() {
        segments.push(build_cue_segment(&group));
    }

    Ok(segments)
}

/// Build one segment JSON object from a non-empty group of cues.
/// Caller guarantees `group` is non-empty and every cue has `text`,
/// `start_ms`, and `end_ms` (validated in `chunk_cues`).
fn build_cue_segment(group: &[&serde_json::Value]) -> serde_json::Value {
    let text = group
        .iter()
        .map(|c| c.get("text").and_then(|v| v.as_str()).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ");
    let first = group[0];
    let last = group[group.len() - 1];
    serde_json::json!({
        "text": text,
        "ts_start": first.get("start").and_then(|v| v.as_str()).unwrap_or(""),
        "ts_end": last.get("end").and_then(|v| v.as_str()).unwrap_or(""),
        "start_ms": first.get("start_ms").and_then(|v| v.as_u64()).unwrap_or(0),
        "end_ms": last.get("end_ms").and_then(|v| v.as_u64()).unwrap_or(0),
        "cue_count": group.len(),
    })
}

#[async_trait]
impl Node for AiChunkNode {
    fn node_type(&self) -> &str {
        "ai_chunk"
    }

    fn description(&self) -> &str {
        "Split text into chunks using fixed-size, delimiter, or subtitle cue strategies"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let mode = config
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("fixed");

        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("ai_chunk requires 'source_key' parameter"))?;
        let source_key = interpolate_ctx(source_key, ctx);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("chunks")
            .to_string();

        let mut output = NodeOutput::new();

        match mode {
            "fixed" | "split" => {
                let text = ctx
                    .get(&source_key)
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk: source_key '{}' not found or not a string in context",
                            source_key
                        )
                    })?;

                let chunks = if mode == "fixed" {
                    let size = config.get("size").and_then(|v| v.as_u64()).unwrap_or(4096) as usize;
                    let delimiters_str = config
                        .get("delimiters")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let prefix = config
                        .get("prefix")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    chunk_fixed(&text, size, delimiters_str.as_bytes(), prefix)
                } else {
                    let delimiters_str = config
                        .get("delimiters")
                        .and_then(|v| v.as_str())
                        .unwrap_or("\n.?");
                    let min_chars = config
                        .get("min_chars")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    chunk_split(&text, delimiters_str.as_bytes(), min_chars)
                };

                let count = chunks.len();
                let chunks_json: Vec<serde_json::Value> =
                    chunks.into_iter().map(serde_json::Value::String).collect();
                output.insert(output_key.clone(), serde_json::Value::Array(chunks_json));
                output.insert(format!("{}_count", output_key), serde_json::json!(count));
            }
            "cues" => {
                let size = config.get("size").and_then(|v| v.as_u64()).unwrap_or(1200) as usize;
                let cues = ctx
                    .get(&source_key)
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk: mode 'cues' requires 'source_key' ('{}') pointing to a cues array",
                            source_key
                        )
                    })?;

                let segments = chunk_cues(cues, size)?;
                let texts: Vec<serde_json::Value> = segments
                    .iter()
                    .map(|s| s.get("text").cloned().unwrap_or(serde_json::Value::Null))
                    .collect();
                let count = segments.len();

                output.insert(output_key.clone(), serde_json::Value::Array(segments));
                output.insert(
                    format!("{}_texts", output_key),
                    serde_json::Value::Array(texts),
                );
                output.insert(format!("{}_count", output_key), serde_json::json!(count));
            }
            other => anyhow::bail!(
                "ai_chunk: unsupported mode '{}' (use 'fixed', 'split', or 'cues')",
                other
            ),
        }

        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );

        Ok(output)
    }
}
