use super::REDACTED;
use super::search::find_first_ascii_case_insensitive;

const SENSITIVE_ASSIGNMENT_KEYS: &[&str] = &[
    "access_token",
    "api-key",
    "api_key",
    "apikey",
    "authorization",
    "client_secret",
    "credential",
    "credentials",
    "passwd",
    "password",
    "pwd",
    "refresh_token",
    "secret",
    "signature",
    "token",
];

pub(super) fn redact_sensitive_assignments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some((relative_start, key)) =
        find_first_ascii_case_insensitive(&text[cursor..], SENSITIVE_ASSIGNMENT_KEYS)
    {
        let start = cursor + relative_start;
        let key_end = start + key.len();
        if !assignment_key_is_bounded(text, start, key_end) {
            output.push_str(&text[cursor..key_end]);
            cursor = key_end;
            continue;
        }
        let Some((separator_start, separator)) = assignment_separator(text, start, key_end) else {
            output.push_str(&text[cursor..key_end]);
            cursor = key_end;
            continue;
        };
        let (content_start, value_end) = assignment_value_bounds(text, separator_start, separator);
        output.push_str(&text[cursor..content_start]);
        output.push_str(REDACTED);
        cursor = value_end;
    }
    output.push_str(&text[cursor..]);
    output
}

fn assignment_key_is_bounded(text: &str, start: usize, key_end: usize) -> bool {
    let before_is_boundary = start == 0
        || text[..start]
            .chars()
            .next_back()
            .is_none_or(|character| !is_assignment_key_character(character));
    let after_is_boundary = text[key_end..]
        .chars()
        .next()
        .is_none_or(|character| !is_assignment_key_character(character));
    before_is_boundary && after_is_boundary
}

fn assignment_separator(text: &str, start: usize, key_end: usize) -> Option<(usize, char)> {
    let enclosing_quote = text[..start]
        .chars()
        .next_back()
        .filter(|character| matches!(character, '\'' | '"'));
    let mut separator_start = key_end;
    if let Some(quote) = enclosing_quote
        && text[separator_start..].starts_with(quote)
    {
        separator_start += quote.len_utf8();
    }
    separator_start = skip_ascii_whitespace(text, separator_start);
    let separator = text[separator_start..].chars().next()?;
    matches!(separator, '=' | ':').then_some((separator_start, separator))
}

fn assignment_value_bounds(text: &str, separator_start: usize, separator: char) -> (usize, usize) {
    let value_start = skip_ascii_whitespace(text, separator_start + separator.len_utf8());
    let quote = text[value_start..]
        .chars()
        .next()
        .filter(|character| matches!(character, '\'' | '"'));
    let content_start = quote
        .map(|character| value_start + character.len_utf8())
        .unwrap_or(value_start);
    let value_end = match quote {
        Some(quote) => quoted_value_end(text, content_start, quote),
        None => text[content_start..]
            .char_indices()
            .find_map(|(offset, character)| {
                (character.is_control()
                    || (separator == '=' && character.is_whitespace())
                    || matches!(character, '&' | ';' | ',' | '\'' | '"' | '<' | '>'))
                .then_some(content_start + offset)
            })
            .unwrap_or(text.len()),
    };
    (content_start, value_end)
}

fn quoted_value_end(text: &str, content_start: usize, quote: char) -> usize {
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

fn skip_ascii_whitespace(text: &str, mut cursor: usize) -> usize {
    while let Some(character) = text[cursor..].chars().next()
        && character.is_ascii_whitespace()
    {
        cursor += character.len_utf8();
    }
    cursor
}

fn is_assignment_key_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}
