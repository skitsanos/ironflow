//! Bounds of the value a sensitive key is assigned, in every form the
//! scrubber recognizes.
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
    /// Index of the first byte of the key word.
    pub(super) start: usize,
    /// Index after the last byte of the key word.
    pub(super) end: usize,
    /// The key names an `Authorization` header, whose value is a scheme and
    /// a credential.
    pub(super) authorization: bool,
    /// The key is a command-line flag such as `--password`, whose value may
    /// be the next argument.
    pub(super) flag: bool,
}

/// How a key is joined to its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Separator {
    /// `=`, which may also be the first half of `==`.
    Equals,
    /// `:`, `:=` or `=>`.
    Other,
    /// Whitespace after a command-line flag.
    Argument,
}

/// The value assigned to `key`, without its quotes or wrapper.
///
/// The key may be quoted and may close a bracket, as a Lua table key does:
/// `["password"] = "x"`. The separator is `=`, `:`, `:=` or `=>`; a flag may
/// instead be followed by its argument, and an XML element by its text
/// (IF-219).
///
/// `None` when no value follows, or when the `=` is the first half of a
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
    if bytes.get(cursor) == Some(&b']') {
        cursor += 1;
    }
    let bare_key = cursor == key.end;
    cursor = skip_blank(text, cursor);
    let (separator, value_start) = match separator_at(bytes, cursor) {
        Some((separator, after)) => (separator, skip_blank(text, after)),
        None if bare_key && is_element(bytes, key) => return element_text(text, key.end + 1),
        None if bare_key && key.flag => (Separator::Argument, argument_start(text, key.end)?),
        None => return None,
    };
    if let Some((quote, content_start)) = Quote::at(bytes, value_start) {
        return Some(content_start..quoted_value_end(text, content_start, quote));
    }
    let first_word = value_start..word_end(text, value_start);
    let comparison = separator == Separator::Equals && &text[first_word.clone()] == "=";
    // A value that is a redacted URL was scrubbed already.
    let redacted_url = first_word.is_empty() && text[value_start..].starts_with(REDACTED_URL);
    if comparison || redacted_url {
        return None;
    }
    if let Some(wrapped) = wrapped_value(text, first_word.clone()) {
        return Some(wrapped);
    }
    Some(value_start..unquoted_value_end(text, first_word, key.authorization))
}

/// The separator at `cursor` and the index after it.
fn separator_at(bytes: &[u8], cursor: usize) -> Option<(Separator, usize)> {
    match (*bytes.get(cursor)?, bytes.get(cursor + 1)) {
        (b'=', Some(b'>')) | (b':', Some(b'=')) => Some((Separator::Other, cursor + 2)),
        (b'=', _) => Some((Separator::Equals, cursor + 1)),
        (b':', _) => Some((Separator::Other, cursor + 1)),
        _ => None,
    }
}

/// Start of the argument after a flag: a gap, then anything but another
/// flag. The gap may hold a line break, as it may before and after the other
/// separators.
fn argument_start(text: &str, flag_end: usize) -> Option<usize> {
    let start = skip_blank(text, flag_end);
    let first = *text.as_bytes().get(start)?;
    (start > flag_end && first != b'-').then_some(start)
}

/// Whether the key is the name of an XML start tag: `<password>`.
fn is_element(bytes: &[u8], key: Key) -> bool {
    bytes.get(key.end) == Some(&b'>') && key.start > 0 && bytes[key.start - 1] == b'<'
}

/// Text of an element after any leading gap, up to the next tag or line
/// end, so indented text on the line after its tag is covered. `None` when
/// empty. Text that already starts with the redaction marker is complete:
/// the line break that ended it in the first pass is a space by now.
fn element_text(text: &str, after_tag: usize) -> Option<Range<usize>> {
    let start = skip_blank(text, after_tag);
    if text[start..].starts_with(REDACTED) {
        return Some(start..start + REDACTED.len());
    }
    let end = text[start..]
        .find(|character: char| character == '<' || character.is_control())
        .map_or(text.len(), |offset| start + offset);
    (end > start).then_some(start..end)
}

/// The quoted content of a value such as `Some("x")` or `Secret('x')`: a
/// word that ends with `(` directly before a quote. Rust's `Debug` output and
/// similar wrappers print a credential that way, and the word alone would
/// leave it in place.
fn wrapped_value(text: &str, word: Range<usize>) -> Option<Range<usize>> {
    let bytes = text.as_bytes();
    let escapes = bytes[word.clone()]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    let quote_start = word.end - escapes;
    if quote_start == word.start || bytes[quote_start - 1] != b'(' {
        return None;
    }
    let (quote, content_start) = Quote::at(bytes, quote_start)?;
    Some(content_start..quoted_value_end(text, content_start, quote))
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

/// Index of the first character at or after `cursor` that is neither
/// whitespace nor a control character. The scrubber turns every control
/// character into a space, so both must read alike or a second pass would
/// see an assignment the first did not.
fn skip_blank(text: &str, cursor: usize) -> usize {
    text[cursor..]
        .find(|character: char| !(character.is_whitespace() || character.is_control()))
        .map_or(text.len(), |offset| cursor + offset)
}
