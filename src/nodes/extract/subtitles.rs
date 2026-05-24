use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::common::{get_path, validate_format};

pub struct ExtractVttNode;

#[async_trait]
impl Node for ExtractVttNode {
    fn node_type(&self) -> &str {
        "extract_vtt"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from WebVTT subtitle files"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = get_path(config, ctx, "extract_vtt")?;
        let format = validate_format(config, "extract_vtt")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("transcript");
        let cues_key = config
            .get("cues_key")
            .and_then(|v| v.as_str())
            .unwrap_or("cues");
        let metadata_key = config.get("metadata_key").and_then(|v| v.as_str());

        let input = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;

        let cues = parse_subtitle_cues(&input, true);
        let content = format_caption_output(&cues, format);
        let transcript = format_caption_output(&cues, "text");
        let metadata = collect_subtitle_metadata(&cues, "vtt");
        let cues_payload = subtitle_cues_as_json(&cues);

        let mut output = NodeOutput::new();
        output.insert(
            "transcript".to_string(),
            serde_json::Value::String(transcript),
        );
        output.insert(cues_key.to_string(), serde_json::to_value(cues_payload)?);
        output.insert(output_key.to_string(), serde_json::Value::String(content));
        if let Some(meta_key) = metadata_key {
            output.insert(meta_key.to_string(), serde_json::to_value(metadata)?);
        }
        Ok(output)
    }
}

pub struct ExtractSrtNode;

#[async_trait]
impl Node for ExtractSrtNode {
    fn node_type(&self) -> &str {
        "extract_srt"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from SRT subtitle files"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = get_path(config, ctx, "extract_srt")?;
        let format = validate_format(config, "extract_srt")?;
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("transcript");
        let cues_key = config
            .get("cues_key")
            .and_then(|v| v.as_str())
            .unwrap_or("cues");
        let metadata_key = config.get("metadata_key").and_then(|v| v.as_str());

        let input = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;

        let cues = parse_subtitle_cues(&input, false);
        let content = format_caption_output(&cues, format);
        let transcript = format_caption_output(&cues, "text");
        let metadata = collect_subtitle_metadata(&cues, "srt");
        let cues_payload = subtitle_cues_as_json(&cues);

        let mut output = NodeOutput::new();
        output.insert(
            "transcript".to_string(),
            serde_json::Value::String(transcript),
        );
        output.insert(cues_key.to_string(), serde_json::to_value(cues_payload)?);
        output.insert(output_key.to_string(), serde_json::Value::String(content));
        if let Some(meta_key) = metadata_key {
            output.insert(meta_key.to_string(), serde_json::to_value(metadata)?);
        }
        Ok(output)
    }
}

#[derive(Clone)]
struct SubtitleCue {
    start_ms: u64,
    end_ms: u64,
    text: String,
}

fn parse_subtitle_cues(contents: &str, is_vtt: bool) -> Vec<SubtitleCue> {
    let mut cues = Vec::new();
    let mut lines = contents.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if is_vtt {
            if trimmed == "WEBVTT" {
                continue;
            }
            if trimmed.starts_with("NOTE") {
                for next in lines.by_ref() {
                    if next.trim().is_empty() {
                        break;
                    }
                }
                continue;
            }
        }

        let Some((start_ms, end_ms)) = parse_caption_range(trimmed) else {
            continue;
        };

        let mut text_lines = Vec::new();
        for candidate in lines.by_ref() {
            if candidate.trim().is_empty() {
                break;
            }
            text_lines.push(candidate.to_string());
        }
        if text_lines.is_empty() {
            continue;
        }

        let raw_text = text_lines
            .into_iter()
            .map(|line| remove_annotation_tags(&line))
            .collect::<Vec<_>>()
            .join(" ");
        let text = raw_text.replace('\u{feff}', "").trim().to_string();
        if !text.is_empty() {
            cues.push(SubtitleCue {
                start_ms,
                end_ms,
                text,
            });
        }
    }

    cues
}

fn parse_caption_range(line: &str) -> Option<(u64, u64)> {
    if line.is_empty() {
        return None;
    }

    if !line.contains("-->") {
        return None;
    }

    let mut parts = line.splitn(2, "-->");
    let start_text = parts.next()?.trim();
    let rest = parts.next()?.trim();
    if rest.is_empty() {
        return None;
    }

    let mut time_parts = rest.split_whitespace();
    let end_text = time_parts.next()?;
    let start_ms = parse_timestamp_ms(start_text)?;
    let end_ms = parse_timestamp_ms(end_text)?;
    if end_ms < start_ms {
        None
    } else {
        Some((start_ms, end_ms))
    }
}

fn parse_timestamp_ms(value: &str) -> Option<u64> {
    let normalized = value.replace(',', ".");
    let mut timestamp_and_ms = normalized.split('.');
    let hms_part = timestamp_and_ms.next()?;
    let ms_part = timestamp_and_ms.next().unwrap_or("000");

    let hms_parts: Vec<u64> = hms_part
        .split(':')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;

    let (hours, minutes, seconds) = match hms_parts.as_slice() {
        [h, m, s] => (*h, *m, *s),
        [m, s] => (0, *m, *s),
        _ => return None,
    };

    if minutes > 59 || seconds > 59 {
        return None;
    }

    let mut millis = ms_part.chars().take(3).collect::<String>();
    while millis.len() < 3 {
        millis.push('0');
    }
    if millis.len() > 3 {
        millis.truncate(3);
    }

    let millis = millis.parse::<u64>().ok()?;
    Some(((hours * 3600 + minutes * 60 + seconds) * 1000) + millis)
}

fn format_timestamp(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let milliseconds = ms % 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{milliseconds:03}")
}

fn subtitle_cues_as_json(cues: &[SubtitleCue]) -> Vec<serde_json::Value> {
    cues.iter()
        .map(|cue| {
            serde_json::json!({
                "start_ms": cue.start_ms,
                "end_ms": cue.end_ms,
                "start": format_timestamp(cue.start_ms),
                "end": format_timestamp(cue.end_ms),
                "text": cue.text,
            })
        })
        .collect()
}

fn format_caption_output(cues: &[SubtitleCue], format: &str) -> String {
    if cues.is_empty() {
        return String::new();
    }

    match format {
        "markdown" => cues
            .iter()
            .map(|cue| {
                format!(
                    "- `{}` -> `{}`: {}",
                    format_timestamp(cue.start_ms),
                    format_timestamp(cue.end_ms),
                    cue.text
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => cues
            .iter()
            .map(|cue| cue.text.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn collect_subtitle_metadata(
    cues: &[SubtitleCue],
    format_name: &str,
) -> BTreeMap<String, serde_json::Value> {
    let mut metadata = BTreeMap::new();
    metadata.insert("type".to_string(), serde_json::json!(format_name));
    let cue_count = u64::try_from(cues.len()).unwrap_or(u64::MAX);
    metadata.insert("cue_count".to_string(), serde_json::json!(cue_count));

    let first_start_ms = cues.first().map_or(0, |cue| cue.start_ms);
    if first_start_ms > 0 {
        metadata.insert(
            "first_start_ms".to_string(),
            serde_json::json!(first_start_ms),
        );
    }
    if let Some(last) = cues.last() {
        metadata.insert("last_end_ms".to_string(), serde_json::json!(last.end_ms));
        metadata.insert(
            "duration_ms".to_string(),
            serde_json::json!(last.end_ms.saturating_sub(first_start_ms)),
        );
    }
    metadata
}

fn remove_annotation_tags(value: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        if ch == '<' {
            in_tag = true;
            continue;
        }
        if ch == '>' {
            in_tag = false;
            continue;
        }
        if !in_tag {
            out.push(ch);
        }
    }
    out
}
