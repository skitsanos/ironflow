use std::collections::BTreeMap;

use anyhow::Result;

use crate::engine::types::{Context, NodeOutput};

use super::super::common::{get_path, validate_format};

pub(super) fn extract(
    config: &serde_json::Value,
    ctx: &Context,
    node_name: &str,
    format_name: &str,
    is_vtt: bool,
) -> Result<NodeOutput> {
    let path = get_path(config, ctx, node_name)?;
    let format = validate_format(config, node_name)?;
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
        .map_err(|error| anyhow::anyhow!("Failed to read '{}': {}", path, error))?;

    let cues = parse_subtitle_cues(&input, is_vtt);
    let mut output = NodeOutput::new();
    output.insert(
        "transcript".to_string(),
        serde_json::Value::String(format_caption_output(&cues, "text")),
    );
    output.insert(
        cues_key.to_string(),
        serde_json::to_value(subtitle_cues_as_json(&cues))?,
    );
    output.insert(
        output_key.to_string(),
        serde_json::Value::String(format_caption_output(&cues, format)),
    );
    if let Some(metadata_key) = metadata_key {
        output.insert(
            metadata_key.to_string(),
            serde_json::to_value(collect_subtitle_metadata(&cues, format_name))?,
        );
    }
    Ok(output)
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
        let text = text_lines
            .into_iter()
            .map(|line| remove_annotation_tags(&line))
            .collect::<Vec<_>>()
            .join(" ")
            .replace('\u{feff}', "")
            .trim()
            .to_string();
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
    if line.is_empty() || !line.contains("-->") {
        return None;
    }
    let mut parts = line.splitn(2, "-->");
    let start = parts.next()?.trim();
    let rest = parts.next()?.trim();
    if rest.is_empty() {
        return None;
    }
    let end = rest.split_whitespace().next()?;
    let (start_ms, end_ms) = (parse_timestamp_ms(start)?, parse_timestamp_ms(end)?);
    (end_ms >= start_ms).then_some((start_ms, end_ms))
}

fn parse_timestamp_ms(value: &str) -> Option<u64> {
    let normalized = value.replace(',', ".");
    let mut timestamp_and_ms = normalized.split('.');
    let hms_part = timestamp_and_ms.next()?;
    let ms_part = timestamp_and_ms.next().unwrap_or("000");
    let hms: Vec<u64> = hms_part
        .split(':')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<_>>()?;
    let (hours, minutes, seconds) = match hms.as_slice() {
        [hours, minutes, seconds] => (*hours, *minutes, *seconds),
        [minutes, seconds] => (0, *minutes, *seconds),
        _ => return None,
    };
    if minutes > 59 || seconds > 59 {
        return None;
    }
    let mut milliseconds = ms_part.chars().take(3).collect::<String>();
    while milliseconds.len() < 3 {
        milliseconds.push('0');
    }
    if milliseconds.len() > 3 {
        milliseconds.truncate(3);
    }
    let milliseconds = milliseconds.parse::<u64>().ok()?;
    Some(((hours * 3600 + minutes * 60 + seconds) * 1000) + milliseconds)
}

fn format_timestamp(milliseconds: u64) -> String {
    let total_seconds = milliseconds / 1000;
    let milliseconds = milliseconds % 1000;
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
    metadata.insert(
        "cue_count".to_string(),
        serde_json::json!(u64::try_from(cues.len()).unwrap_or(u64::MAX)),
    );
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
    let mut output = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
}
