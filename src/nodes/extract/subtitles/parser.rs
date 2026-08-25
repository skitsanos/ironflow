use anyhow::Result;

use crate::artifacts::FileSource;
use crate::engine::types::{Context, NodeOutput};
use crate::util::execution::{ExecutionControl, run_tracked_blocking_step};

use super::super::common::{ensure_distinct_keys, optional_string, string_or, validate_format};
use super::super::resource::{Budget, Limits, read_string};
use super::output::{collect_metadata, cues_as_json, format_output};
use crate::util::file_source::get_file_source;

pub(super) async fn extract(
    config: &serde_json::Value,
    ctx: &Context,
    node_name: &'static str,
    format_name: &'static str,
    is_vtt: bool,
) -> Result<NodeOutput> {
    let source = get_file_source(config, ctx, node_name)?;
    let format = validate_format(config, node_name)?.to_string();
    let output_key = string_or(config, "output_key", "transcript", node_name)?.to_string();
    let cues_key = string_or(config, "cues_key", "cues", node_name)?.to_string();
    let metadata_key = optional_string(config, "metadata_key", node_name)?.map(str::to_string);
    if format == "markdown" && output_key == "transcript" {
        anyhow::bail!(
            "{node_name}: format 'markdown' requires an output_key distinct from 'transcript'"
        );
    }
    let mut keys = vec![
        ("transcript", "transcript"),
        ("cues_key", cues_key.as_str()),
    ];
    if output_key != "transcript" {
        keys.push(("output_key", output_key.as_str()));
    }
    if let Some(metadata_key) = metadata_key.as_deref() {
        keys.push(("metadata_key", metadata_key));
    }
    ensure_distinct_keys(node_name, &keys)?;

    run_tracked_blocking_step(move |execution| {
        extract_subtitles(SubtitleRequest {
            source,
            format,
            output_key,
            cues_key,
            metadata_key,
            node_name,
            format_name,
            is_vtt,
            execution: &execution,
        })
    })
    .await
}

struct SubtitleRequest<'a> {
    source: FileSource,
    format: String,
    output_key: String,
    cues_key: String,
    metadata_key: Option<String>,
    node_name: &'static str,
    format_name: &'static str,
    is_vtt: bool,
    execution: &'a ExecutionControl,
}

fn extract_subtitles(request: SubtitleRequest<'_>) -> Result<NodeOutput> {
    let limits = Limits::current();
    let mut budget = Budget::new(request.node_name, limits, request.execution);
    let input = read_string(
        &request.source,
        crate::util::limits::max_file_bytes(),
        request.node_name,
        request.execution,
    )?;

    let cues = parse_subtitle_cues(&input, request.is_vtt, request.node_name, &mut budget)?;
    let transcript = format_output(&cues, "text", &mut budget)?;
    let formatted = if request.output_key == "transcript" {
        None
    } else {
        Some(format_output(&cues, &request.format, &mut budget)?)
    };
    let cue_values = cues_as_json(&cues, &mut budget)?;

    let mut output = NodeOutput::new();
    output.insert(
        "transcript".to_string(),
        serde_json::Value::String(transcript),
    );
    output.insert(request.cues_key, serde_json::to_value(cue_values)?);
    if let Some(formatted) = formatted {
        output.insert(request.output_key, serde_json::Value::String(formatted));
    }
    if let Some(metadata_key) = request.metadata_key {
        output.insert(
            metadata_key,
            serde_json::to_value(collect_metadata(&cues, request.format_name))?,
        );
    }
    budget.ensure_output(&output)?;
    Ok(output)
}

pub(super) struct SubtitleCue {
    pub(super) start_ms: u64,
    pub(super) end_ms: u64,
    pub(super) text: String,
    /// Speaker from a WebVTT voice span (`<v Alice>`), when the cue carries one.
    /// `None` for SRT and for WebVTT without voice spans, so unlabelled captions
    /// behave exactly as before.
    pub(super) speaker: Option<String>,
}

fn parse_subtitle_cues(
    contents: &str,
    is_vtt: bool,
    node_name: &str,
    budget: &mut Budget<'_>,
) -> Result<Vec<SubtitleCue>> {
    let mut cues = Vec::new();
    let mut lines = contents.lines().enumerate().peekable();
    while let Some((line_index, line)) = lines.next() {
        budget.charge_item("subtitle input lines")?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_vtt {
            if trimmed == "WEBVTT" {
                continue;
            }
            if trimmed == "NOTE" || trimmed.starts_with("NOTE ") || trimmed.starts_with("NOTE\t") {
                for (_, next) in lines.by_ref() {
                    budget.charge_item("subtitle input lines")?;
                    if next.trim().is_empty() {
                        break;
                    }
                }
                continue;
            }
        }

        let Some((start_ms, end_ms)) = parse_caption_range(trimmed).map_err(|error| {
            anyhow::anyhow!(
                "{node_name}: invalid cue timing at line {}: {error}",
                line_index + 1
            )
        })?
        else {
            continue;
        };
        let mut text = String::new();
        let mut speaker: Option<String> = None;
        for (_, candidate) in lines.by_ref() {
            budget.charge_item("subtitle input lines")?;
            if candidate.trim().is_empty() {
                break;
            }
            // Read the voice span before the tags are stripped; a cue is
            // attributed to the first speaker it names.
            if is_vtt && speaker.is_none() {
                speaker = extract_voice_name(candidate);
            }
            let cleaned = remove_annotation_tags(candidate, budget)?;
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&cleaned);
        }
        let text = text.replace('\u{feff}', "").trim().to_string();
        if !text.is_empty() {
            budget.charge_item("subtitle cue count")?;
            cues.push(SubtitleCue {
                start_ms,
                end_ms,
                text,
                speaker,
            });
        }
    }
    Ok(cues)
}

fn parse_caption_range(line: &str) -> Result<Option<(u64, u64)>> {
    if line.is_empty() || !line.contains("-->") {
        return Ok(None);
    }
    let mut parts = line.splitn(2, "-->");
    let start = parts.next().unwrap_or_default().trim();
    let rest = parts.next().unwrap_or_default().trim();
    if rest.is_empty() {
        anyhow::bail!("missing cue end timestamp");
    }
    let end = rest.split_whitespace().next().unwrap_or_default();
    let start_ms = parse_timestamp_ms(start)
        .ok_or_else(|| anyhow::anyhow!("invalid start timestamp '{start}'"))?;
    let end_ms =
        parse_timestamp_ms(end).ok_or_else(|| anyhow::anyhow!("invalid end timestamp '{end}'"))?;
    if end_ms < start_ms {
        anyhow::bail!("cue end precedes its start");
    }
    Ok(Some((start_ms, end_ms)))
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
    hours
        .checked_mul(3600)?
        .checked_add(minutes.checked_mul(60)?)?
        .checked_add(seconds)?
        .checked_mul(1000)?
        .checked_add(milliseconds)
}

/// Extract the speaker from a WebVTT voice span.
///
/// Handles `<v Alice>`, `<v.loud Alice>` and `<v.first.loud Alice Smith>`; the
/// class list is attached to `v` with dots and the remainder is the speaker.
/// Returns `None` for any other tag, so `<i>`, `<b>` and friends are ignored,
/// and for a voice span carrying only classes and no name.
fn extract_voice_name(value: &str) -> Option<String> {
    let start = value.find("<v")?;
    let rest = &value[start + 2..];
    // `<video>` must not be mistaken for a voice span: the tag name ends here.
    let boundary = rest.chars().next()?;
    if !boundary.is_whitespace() && boundary != '.' && boundary != '>' {
        return None;
    }
    let end = rest.find('>')?;
    let annotation = &rest[..end];
    let name = if annotation.starts_with('.') {
        match annotation.find(char::is_whitespace) {
            Some(index) => annotation[index..].trim(),
            None => "",
        }
    } else {
        annotation.trim()
    };
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn remove_annotation_tags(value: &str, budget: &Budget<'_>) -> Result<String> {
    let mut output = String::new();
    let mut in_tag = false;
    for (index, character) in value.chars().enumerate() {
        if index % 4096 == 0 {
            budget.checkpoint()?;
        }
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    Ok(output)
}

#[cfg(test)]
mod voice_span_tests {
    use super::*;

    // run_blocking_step needs 'static, so the fixture is moved in rather than
    // borrowed.
    async fn parse(contents: &'static str, is_vtt: bool) -> Vec<SubtitleCue> {
        crate::util::execution::run_blocking_step(move |execution| {
            let limits = Limits {
                max_output_bytes: 1_000_000,
                max_items: 10_000,
                max_zip_entries: 10,
                max_zip_bytes: 10,
                max_pdf_pages: 10,
            };
            let mut budget = Budget::new("test", limits, &execution);
            parse_subtitle_cues(contents, is_vtt, "extract_vtt", &mut budget)
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn voice_span_names_the_speaker_and_leaves_the_text_clean() {
        let cues = parse(
            "WEBVTT\n\n1\n00:00:00.000 --> 00:00:02.000\n<v Alice>Hello there</v>\n",
            true,
        )
        .await;
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].speaker.as_deref(), Some("Alice"));
        assert_eq!(cues[0].text, "Hello there");
    }

    #[tokio::test]
    async fn classes_are_not_part_of_the_speaker_name() {
        let cues = parse(
            "WEBVTT\n\n1\n00:00:00.000 --> 00:00:02.000\n<v.first.loud Bob Smith>Hi</v>\n",
            true,
        )
        .await;
        assert_eq!(cues[0].speaker.as_deref(), Some("Bob Smith"));
    }

    #[tokio::test]
    async fn captions_without_a_voice_span_stay_unlabelled() {
        // The unlabelled path has to keep working: a caption-only transcript
        // still parses, with no speaker rather than an empty one.
        let cues = parse(
            "WEBVTT\n\n1\n00:00:00.000 --> 00:00:02.000\nJust a caption\n",
            true,
        )
        .await;
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].speaker, None);
        assert_eq!(cues[0].text, "Just a caption");
    }

    #[tokio::test]
    async fn other_markup_is_not_mistaken_for_a_voice_span() {
        let cues = parse(
            "WEBVTT\n\n1\n00:00:00.000 --> 00:00:02.000\n<i>emphasis</i> and <video> too\n",
            true,
        )
        .await;
        assert_eq!(cues[0].speaker, None);
    }

    #[tokio::test]
    async fn srt_never_reports_a_speaker() {
        let cues = parse("1\n00:00:00,000 --> 00:00:02,000\n<v Alice>Hello\n", false).await;
        assert_eq!(cues[0].speaker, None);
    }
}
