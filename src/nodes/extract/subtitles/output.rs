use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::Result;

use super::super::resource::Budget;
use super::parser::SubtitleCue;

pub(super) fn cues_as_json(
    cues: &[SubtitleCue],
    budget: &mut Budget<'_>,
) -> Result<Vec<serde_json::Value>> {
    let mut values = Vec::new();
    values.try_reserve(cues.len())?;
    for cue in cues {
        budget.checkpoint()?;
        budget.charge_output(
            (cue.text.len() as u64).saturating_add(96),
            "subtitle cue output",
        )?;
        let mut value = serde_json::json!({
            "start_ms": cue.start_ms,
            "end_ms": cue.end_ms,
            "start": format_timestamp(cue.start_ms),
            "end": format_timestamp(cue.end_ms),
            "text": cue.text,
        });
        // Only present when the cue named a speaker, so consumers can tell a
        // labelled transcript from an unlabelled one rather than seeing an
        // empty string for both.
        if let Some(speaker) = &cue.speaker {
            budget.charge_output(speaker.len() as u64, "subtitle cue speaker")?;
            value["speaker"] = serde_json::Value::String(speaker.clone());
        }
        values.push(value);
    }
    Ok(values)
}

pub(super) fn format_output(
    cues: &[SubtitleCue],
    format: &str,
    budget: &mut Budget<'_>,
) -> Result<String> {
    let mut output = String::new();
    for (index, cue) in cues.iter().enumerate() {
        let separator_bytes = usize::from(index > 0);
        let cue_bytes = if format == "markdown" {
            12_usize
                .saturating_add(timestamp_len(cue.start_ms))
                .saturating_add(timestamp_len(cue.end_ms))
                .saturating_add(cue.text.len())
        } else {
            cue.text.len()
        };
        let rendered_bytes = separator_bytes.saturating_add(cue_bytes);
        budget.charge_output(rendered_bytes as u64, "subtitle transcript output")?;
        output.try_reserve_exact(rendered_bytes)?;

        if separator_bytes > 0 {
            output.push('\n');
        }
        if format == "markdown" {
            write!(
                output,
                "- `{}` -> `{}`: {}",
                format_timestamp(cue.start_ms),
                format_timestamp(cue.end_ms),
                cue.text
            )?;
        } else {
            output.push_str(&cue.text);
        }
    }
    Ok(output)
}

pub(super) fn collect_metadata(
    cues: &[SubtitleCue],
    format_name: &str,
) -> BTreeMap<String, serde_json::Value> {
    let mut metadata = BTreeMap::new();
    metadata.insert("type".to_string(), serde_json::json!(format_name));
    metadata.insert("cue_count".to_string(), serde_json::json!(cues.len()));
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

fn format_timestamp(milliseconds: u64) -> String {
    let total_seconds = milliseconds / 1000;
    let milliseconds = milliseconds % 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{milliseconds:03}")
}

fn timestamp_len(milliseconds: u64) -> usize {
    let hours = milliseconds / 3_600_000;
    decimal_digits(hours).max(2).saturating_add(10)
}

fn decimal_digits(mut value: u64) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}
