use serde_json::Value;

use crate::engine::types::Context;

use super::InterpolationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Segment {
    Key(String),
    Index(usize),
}

pub(super) fn parse(expression: &str) -> Result<Vec<Segment>, InterpolationError> {
    let bytes = expression.as_bytes();
    if !expression.starts_with("ctx") {
        return Err(error(expression, "the path must start with `ctx`"));
    }

    let mut cursor = 3;
    let mut segments = Vec::new();

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'.' => {
                cursor += 1;
                let start = cursor;
                if cursor >= bytes.len() || !is_identifier_start(bytes[cursor]) {
                    return Err(error(
                        expression,
                        "a dot must be followed by an ASCII identifier",
                    ));
                }
                cursor += 1;
                while cursor < bytes.len() && is_identifier_continue(bytes[cursor]) {
                    cursor += 1;
                }
                segments.push(Segment::Key(expression[start..cursor].to_string()));
            }
            b'[' => {
                let (segment, next) = parse_bracket_selector(expression, cursor)?;
                segments.push(segment);
                cursor = next;
            }
            _ => {
                return Err(error(
                    expression,
                    "expected `.` or `[` after a path selector; expressions and fallbacks are unsupported",
                ));
            }
        }
    }

    if segments.is_empty() {
        return Err(error(
            expression,
            "`ctx` must be followed by at least one object-key selector",
        ));
    }
    if matches!(segments.first(), Some(Segment::Index(_))) {
        return Err(error(
            expression,
            "the context root is an object; its first selector must be an object key",
        ));
    }

    Ok(segments)
}

fn parse_bracket_selector(
    expression: &str,
    open: usize,
) -> Result<(Segment, usize), InterpolationError> {
    let bytes = expression.as_bytes();
    let mut cursor = open + 1;
    if cursor >= bytes.len() {
        return Err(error(
            expression,
            "an array or quoted-key selector is unclosed",
        ));
    }

    if bytes[cursor] == b'"' {
        let literal_start = cursor;
        cursor += 1;
        let mut escaped = false;
        let mut closed = false;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'"' if !escaped => {
                    cursor += 1;
                    closed = true;
                    break;
                }
                b'\\' if !escaped => escaped = true,
                _ => escaped = false,
            }
            cursor += 1;
        }
        if !closed {
            return Err(error(expression, "a quoted object key is unclosed"));
        }

        let key: String = serde_json::from_str(&expression[literal_start..cursor])
            .map_err(|_| error(expression, "the bracketed object key is not a JSON string"))?;
        if cursor >= bytes.len() || bytes[cursor] != b']' {
            return Err(error(
                expression,
                "a quoted object key must be followed immediately by `]`",
            ));
        }
        return Ok((Segment::Key(key), cursor + 1));
    }

    let index_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    if cursor == index_start {
        return Err(error(
            expression,
            "array indexes must be non-negative decimal integers",
        ));
    }
    if cursor >= bytes.len() || bytes[cursor] != b']' {
        return Err(error(
            expression,
            "an array index must be followed immediately by `]`",
        ));
    }

    let digits = &expression[index_start..cursor];
    if digits.len() > 1 && digits.starts_with('0') {
        return Err(error(
            expression,
            "array indexes must use canonical decimal notation without leading zeros",
        ));
    }
    let index = digits
        .parse::<usize>()
        .map_err(|_| error(expression, "the array index is too large"))?;
    Ok((Segment::Index(index), cursor + 1))
}

pub(super) fn resolve<'a>(segments: &[Segment], context: &'a Context) -> Option<&'a Value> {
    let (first, rest) = segments.split_first()?;
    let Segment::Key(first) = first else {
        return None;
    };

    let mut current = context.get(first)?;
    for segment in rest {
        current = match segment {
            Segment::Key(key) => current.as_object()?.get(key)?,
            Segment::Index(index) => current.as_array()?.get(*index)?,
        };
    }
    Some(current)
}

pub(super) fn is_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    is_identifier_start(first) && bytes.all(is_identifier_continue)
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn error(expression: &str, detail: &str) -> InterpolationError {
    InterpolationError::new(format!("${{{expression}}}"), detail)
}
