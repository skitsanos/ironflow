//! Bounds of the value in a `key=value` or `key: value` assignment.
//!
//! Every function reads forward from the key, and the caller resumes after
//! the value, so a text is read a constant number of times.

use std::ops::Range;

use super::quoted::{Quote, quoted_value_end};
use super::{REDACTED, REDACTED_URL};

/// Authentication schemes whose credential is the next word.
const AUTH_SCHEMES: &[&str] = &["bearer", "basic"];

/// What the scan knows about the key of an assignment.
#[derive(Clone, Copy)]
pub(super) struct Key {
    /// Index after the last byte of the key word.
    pub(super) end: usize,
    /// The key names an `Authorization` header, whose value is a scheme and
    /// a credential.
    pub(super) authorization: bool,
}

/// The value assigned to `key`, without its quotes. `None` when the key is
/// not followed by `=` or `:`, or when the `=` is the first half of a
/// comparison operator: `key == value` would otherwise have its second `=`
/// redacted, which hides nothing and breaks the expression. Only an unquoted
/// value that is exactly `=` is read that way, so a value that merely starts
/// with `=` is still redacted.
pub(super) fn assigned_value(text: &str, key: Key) -> Option<Range<usize>> {
    let bytes = text.as_bytes();
    let mut cursor = key.end;
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
    if let Some((quote, content_start)) = Quote::at(bytes, value_start) {
        return Some(content_start..quoted_value_end(text, content_start, quote));
    }
    let first_word = value_start..word_end(text, value_start);
    let comparison = separator == b'=' && &text[first_word.clone()] == "=";
    // A value that is a redacted URL was scrubbed already.
    let redacted_url = first_word.is_empty() && text[value_start..].starts_with(REDACTED_URL);
    if comparison || redacted_url {
        return None;
    }
    Some(value_start..unquoted_value_end(text, first_word, key.authorization))
}

/// End of an unquoted value that starts with `first_word`.
///
/// A credential is one word, after `=` and after `:` alike: reading a header
/// value to the next comma took the ordinary words that followed it on the
/// line (IF-219). The value is two words when the first is an authentication
/// scheme or the key is an `Authorization` header, whose credential follows
/// its scheme. A value that is already the redaction marker is complete, so
/// a second pass takes no further word.
fn unquoted_value_end(text: &str, first_word: Range<usize>, authorization: bool) -> usize {
    let word = &text[first_word.clone()];
    let is_scheme = AUTH_SCHEMES
        .iter()
        .any(|scheme| word.eq_ignore_ascii_case(scheme));
    if word.starts_with(REDACTED) || !(authorization || is_scheme) {
        return first_word.end;
    }
    let credential_start = first_word.end
        + text[first_word.end..]
            .bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count();
    word_end(text, credential_start)
}

/// End of the word at `start`: the next whitespace, control character or
/// delimiter. The marker that stands for a redacted URL is never part of a
/// word; taking it would let the URL before it run on in a later pass.
fn word_end(text: &str, start: usize) -> usize {
    let end = text[start..]
        .char_indices()
        .find_map(|(offset, character)| {
            (character.is_control()
                || character.is_whitespace()
                || matches!(character, '&' | ';' | ',' | '\'' | '"' | '<' | '>'))
            .then_some(start + offset)
        })
        .unwrap_or(text.len());
    // The marker contains a space, so a word that reaches it ends in its
    // first half.
    let (marker_head, marker_tail) = REDACTED_URL.split_at(REDACTED_URL.len() - " URL]".len());
    if text[start..end].ends_with(marker_head) && text[end..].starts_with(marker_tail) {
        return end - marker_head.len();
    }
    end
}

fn skip_ascii_whitespace(bytes: &[u8], cursor: usize) -> usize {
    cursor
        + bytes[cursor..]
            .iter()
            .take_while(|byte| byte.is_ascii_whitespace())
            .count()
}
