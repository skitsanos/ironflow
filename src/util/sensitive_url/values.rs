//! Bounds of the value in a `key=value` or `key: value` assignment.
//!
//! Every function reads forward from the key, and the caller resumes after
//! the value, so a text is read a constant number of times.

use std::ops::Range;

/// Authentication schemes whose credential is the next word.
const AUTH_SCHEMES: &[&str] = &["bearer", "basic"];

/// A quote with the backslashes that escape it. JSON serialized inside a JSON
/// string writes its quotes as `\"`, and as `\\\"` one level deeper.
#[derive(Clone, Copy)]
struct Quote {
    mark: u8,
    escapes: usize,
}

impl Quote {
    /// The quote that starts at `index`, with the index after it.
    fn at(bytes: &[u8], index: usize) -> Option<(Self, usize)> {
        let escapes = bytes
            .get(index..)?
            .iter()
            .take_while(|byte| **byte == b'\\')
            .count();
        let mark = *bytes.get(index + escapes)?;
        matches!(mark, b'\'' | b'"').then_some((Self { mark, escapes }, index + escapes + 1))
    }
}

/// The value assigned to the key that ends at `key_end`, without its quotes.
/// `None` when the key is not followed by `=` or `:`.
pub(super) fn assigned_value(text: &str, key_end: usize) -> Option<Range<usize>> {
    let bytes = text.as_bytes();
    let mut cursor = key_end;
    // A quoted key: step over its closing quote, however it is escaped.
    if let Some((_, after)) = Quote::at(bytes, cursor) {
        cursor = after;
    }
    cursor = skip_ascii_whitespace(bytes, cursor);
    let separator = *bytes.get(cursor)?;
    if !matches!(separator, b'=' | b':') {
        return None;
    }
    let value_start = skip_ascii_whitespace(bytes, cursor + 1);
    Some(match Quote::at(bytes, value_start) {
        Some((quote, content_start)) => content_start..quoted_value_end(text, content_start, quote),
        None => value_start..unquoted_value_end(text, value_start, separator == b'='),
    })
}

fn quoted_value_end(text: &str, content_start: usize, quote: Quote) -> usize {
    if quote.escapes == 0 {
        plain_quoted_value_end(text, content_start, char::from(quote.mark))
    } else {
        escaped_quoted_value_end(text, content_start, quote)
    }
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

/// End of an unquoted value. After `=` it is one word, or an authentication
/// scheme and the credential that follows it; after `:` it runs to the next
/// delimiter, as a header value does.
fn unquoted_value_end(text: &str, start: usize, single_word: bool) -> usize {
    let end = delimiter_index(text, start, single_word);
    let is_scheme = AUTH_SCHEMES
        .iter()
        .any(|scheme| text[start..end].eq_ignore_ascii_case(scheme));
    if single_word && is_scheme {
        let credential_start = end
            + text[end..]
                .bytes()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
        return delimiter_index(text, credential_start, true);
    }
    end
}

fn delimiter_index(text: &str, start: usize, stop_at_whitespace: bool) -> usize {
    text[start..]
        .char_indices()
        .find_map(|(offset, character)| {
            (character.is_control()
                || (stop_at_whitespace && character.is_whitespace())
                || matches!(character, '&' | ';' | ',' | '\'' | '"' | '<' | '>'))
            .then_some(start + offset)
        })
        .unwrap_or(text.len())
}

fn skip_ascii_whitespace(bytes: &[u8], cursor: usize) -> usize {
    cursor
        + bytes[cursor..]
            .iter()
            .take_while(|byte| byte.is_ascii_whitespace())
            .count()
}
