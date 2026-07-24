use anyhow::Result;

/// Group ordered subtitle cues into size-bounded timestamped segments.
pub(super) fn chunk_cues(
    cues: &[serde_json::Value],
    size: usize,
) -> Result<Vec<serde_json::Value>> {
    let mut segments = Vec::new();
    let mut group = Vec::new();
    let mut group_chars = 0;
    for (index, cue) in cues.iter().enumerate() {
        let object = cue
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("ai_chunk: cue at index {} is not an object", index))?;
        let text = object
            .get("text")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "ai_chunk: cue at index {} is missing a string 'text' field",
                    index
                )
            })?;
        if object
            .get("start_ms")
            .and_then(serde_json::Value::as_u64)
            .is_none()
        {
            anyhow::bail!(
                "ai_chunk: cue at index {} is missing numeric 'start_ms'",
                index
            );
        }
        if object
            .get("end_ms")
            .and_then(serde_json::Value::as_u64)
            .is_none()
        {
            anyhow::bail!(
                "ai_chunk: cue at index {} is missing numeric 'end_ms'",
                index
            );
        }
        let cue_chars = text.chars().count();
        let added = cue_chars + usize::from(!group.is_empty());
        if !group.is_empty() && group_chars + added > size {
            segments.push(build_cue_segment(&group));
            group.clear();
            group_chars = 0;
        }
        group_chars += cue_chars + usize::from(!group.is_empty());
        group.push(cue);
    }
    if !group.is_empty() {
        segments.push(build_cue_segment(&group));
    }
    Ok(segments)
}

fn build_cue_segment(group: &[&serde_json::Value]) -> serde_json::Value {
    let text = group
        .iter()
        .map(|cue| {
            cue.get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let first = group[0];
    let last = group[group.len() - 1];
    serde_json::json!({
        "text": text,
        "ts_start": first.get("start").and_then(serde_json::Value::as_str).unwrap_or(""),
        "ts_end": last.get("end").and_then(serde_json::Value::as_str).unwrap_or(""),
        "start_ms": first.get("start_ms").and_then(serde_json::Value::as_u64).unwrap_or(0),
        "end_ms": last.get("end_ms").and_then(serde_json::Value::as_u64).unwrap_or(0),
        "cue_count": group.len(),
    })
}
