/// Fixed-size chunking over UTF-8-safe byte windows.
pub(super) fn chunk_fixed(text: &str, size: usize, delimiters: &str, prefix: bool) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let size = size.max(1);
    let mut chunks = Vec::new();
    let mut position = 0;
    while position < text.len() {
        let remaining = text.len() - position;
        if remaining <= size {
            chunks.push(text[position..].to_string());
            break;
        }
        let end = fixed_window_end(text, position, size);
        let window = &text[position..end];
        if let Some((relative, delimiter_len)) = find_last_delimiter(window, delimiters) {
            let delimiter = position + relative;
            if delimiter == position {
                chunks.push(text[position..end].to_string());
                position = end;
            } else if prefix {
                chunks.push(text[position..delimiter].to_string());
                position = delimiter;
            } else {
                let split = delimiter + delimiter_len;
                chunks.push(text[position..split].to_string());
                position = split;
            }
        } else {
            chunks.push(text[position..end].to_string());
            position = end;
        }
    }
    chunks
}

fn find_last_delimiter(window: &str, delimiters: &str) -> Option<(usize, usize)> {
    if delimiters.is_empty() {
        return None;
    }
    window
        .char_indices()
        .rev()
        .find(|(_, character)| delimiters.contains(*character))
        .map(|(offset, character)| (offset, character.len_utf8()))
}

fn fixed_window_end(text: &str, position: usize, size: usize) -> usize {
    let mut end = position.saturating_add(size).min(text.len());
    while end > position && !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == position {
        text[position..]
            .chars()
            .next()
            .map_or(text.len(), |character| position + character.len_utf8())
    } else {
        end
    }
}
