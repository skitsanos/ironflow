/// Split at delimiter characters and merge adjacent undersized segments.
pub(super) fn chunk_split(text: &str, delimiters: &str, min_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    if delimiters.is_empty() {
        return vec![text.to_string()];
    }
    let mut segments = Vec::new();
    let mut start = 0;
    for (offset, character) in text.char_indices() {
        if delimiters.contains(character) {
            let end = offset + character.len_utf8();
            segments.push((start, end, text[start..end].chars().count()));
            start = end;
        }
    }
    if start < text.len() {
        segments.push((start, text.len(), text[start..].chars().count()));
    }

    if min_chars > 0 && segments.len() > 1 {
        let mut merged: Vec<(usize, usize, usize)> = Vec::new();
        for (start, end, character_count) in segments {
            if let Some(previous) = merged.last_mut()
                && (previous.2 < min_chars || character_count < min_chars)
            {
                previous.1 = end;
                previous.2 += character_count;
                continue;
            }
            merged.push((start, end, character_count));
        }
        segments = merged;
    }
    segments
        .into_iter()
        .map(|(start, end, _)| text[start..end].to_string())
        .collect()
}
