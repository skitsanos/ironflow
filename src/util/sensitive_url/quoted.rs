//! Quoted values: plain quotes, and quotes escaped by backslashes as JSON
//! serialized inside a JSON string writes them.

use super::REDACTED;

/// A quote with the backslashes that escape it. JSON serialized inside a JSON
/// string writes its quotes as `\"`, and as `\\\"` one level deeper.
#[derive(Clone, Copy)]
pub(super) struct Quote {
    mark: u8,
    escapes: usize,
}

impl Quote {
    /// The quote that starts at `index`, with the index after it.
    pub(super) fn at(bytes: &[u8], index: usize) -> Option<(Self, usize)> {
        let escapes = bytes
            .get(index..)?
            .iter()
            .take_while(|byte| **byte == b'\\')
            .count();
        let mark = *bytes.get(index + escapes)?;
        matches!(mark, b'\'' | b'"').then_some((Self { mark, escapes }, index + escapes + 1))
    }
}

/// End of the quoted value whose content starts at `content_start`.
///
/// A value that already starts with the redaction marker was scrubbed
/// before. When its quote never closed, the first pass ended it at a line
/// break that is now a space, so it ends at the first whitespace here too
/// and a second pass takes nothing more (IF-219).
pub(super) fn quoted_value_end(text: &str, content_start: usize, quote: Quote) -> usize {
    let end = if quote.escapes == 0 {
        plain_quoted_value_end(text, content_start, char::from(quote.mark))
    } else {
        escaped_quoted_value_end(text, content_start, quote)
    };
    if !text[content_start..].starts_with(REDACTED) {
        return end;
    }
    text[content_start..end]
        .find(char::is_whitespace)
        .map_or(end, |offset| content_start + offset)
}

/// End of a value in plain quotes. A backslash escapes the next character and
/// a doubled quote is a literal quote, as in a DSN or SQL string.
fn plain_quoted_value_end(text: &str, content_start: usize, quote: char) -> usize {
    let mut cursor = content_start;
    let mut escaped = false;
    while let Some(character) = text[cursor..].chars().next() {
        if character.is_control() {
            break;
        }
        if escaped {
            escaped = false;
            cursor += character.len_utf8();
            continue;
        }
        if character == '\\' {
            escaped = true;
            cursor += character.len_utf8();
            continue;
        }
        if character == quote {
            let next = cursor + character.len_utf8();
            if text[next..].starts_with(quote) {
                cursor = next + quote.len_utf8();
                continue;
            }
            break;
        }
        cursor += character.len_utf8();
    }
    cursor
}

/// End of a value opened by an escaped quote: the next quote escaped by the
/// same number of backslashes. A quote escaped differently belongs to the
/// value, and a value that never closes runs to the first control character
/// or the end of the text, so the scan can over-redact but not stop early.
fn escaped_quoted_value_end(text: &str, content_start: usize, quote: Quote) -> usize {
    let mark = char::from(quote.mark);
    let mut run_start = content_start;
    let mut run = 0;
    for (offset, character) in text[content_start..].char_indices() {
        let index = content_start + offset;
        if character.is_control() {
            return index;
        }
        if character == '\\' {
            if run == 0 {
                run_start = index;
            }
            run += 1;
            continue;
        }
        if character == mark && run == quote.escapes {
            return run_start;
        }
        run = 0;
    }
    text.len()
}
