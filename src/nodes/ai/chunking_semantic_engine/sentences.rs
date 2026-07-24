pub(crate) fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut index = 0;

    while index < len {
        if matches!(bytes[index], b'.' | b'!' | b'?')
            && (index + 1 >= len || bytes[index + 1].is_ascii_whitespace())
        {
            let end = index + 1;
            let sentence = &text[start..end];
            if !sentence.trim().is_empty() {
                sentences.push(sentence.to_string());
            }
            index = end;
            while index < len && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            start = index;
        } else {
            index += 1;
        }
    }

    if start < len {
        let remainder = &text[start..];
        if !remainder.trim().is_empty() {
            sentences.push(remainder.to_string());
        }
    }
    sentences
}

pub(crate) fn group_sentences_at_boundaries(
    sentences: &[String],
    split_indices: &[usize],
) -> Vec<String> {
    if sentences.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut chunk_start = 0;
    for &split_index in split_indices {
        let chunk_end = split_index + 1;
        if chunk_end > chunk_start && chunk_end <= sentences.len() {
            chunks.push(sentences[chunk_start..chunk_end].join(" "));
            chunk_start = chunk_end;
        }
    }
    if chunk_start < sentences.len() {
        chunks.push(sentences[chunk_start..].join(" "));
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_supported_sentence_terminators() {
        assert_eq!(
            split_sentences("First. Second! Third? Tail"),
            vec!["First.", "Second!", "Third?", "Tail"]
        );
    }

    #[test]
    fn groups_sentences_at_valid_boundaries() {
        let sentences = vec!["A.".into(), "B.".into(), "C.".into()];
        assert_eq!(
            group_sentences_at_boundaries(&sentences, &[0]),
            vec!["A.", "B. C."]
        );
    }
}
