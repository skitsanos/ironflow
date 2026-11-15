//! Integer literals that JSON text can carry but a parsed value cannot
//! (IF-223).
//!
//! `serde_json` holds an integer in `i64::MIN..=u64::MAX` exactly and reads
//! any other integer literal as the nearest float, so a 30-digit identifier
//! comes back rounded. Such a literal is kept as a string of its digits
//! instead, the rule `csv_parse` and `yaml_parse` follow. Numbers written
//! with a fraction or an exponent are floats by their own spelling and are
//! left to the parser.

use std::borrow::Cow;

/// `text` with every integer literal outside `i64::MIN..=u64::MAX` put in
/// quotes. Text without one is returned as it is. The text is not
/// validated; a document that is not JSON is still not JSON afterwards.
pub fn quote_wide_integers(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut quoted: Option<String> = None;
    let mut copied = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => index = string_end(bytes, index),
            b'-' | b'0'..=b'9' => {
                let start = index;
                while index < bytes.len() && is_number_byte(bytes[index]) {
                    index += 1;
                }
                let token = &text[start..index];
                if is_wide_integer(token) {
                    let quoted = quoted.get_or_insert_with(|| String::with_capacity(text.len()));
                    quoted.push_str(&text[copied..start]);
                    quoted.push('"');
                    quoted.push_str(token);
                    quoted.push('"');
                    copied = index;
                }
            }
            _ => index += 1,
        }
    }
    match quoted {
        Some(mut quoted) => {
            quoted.push_str(&text[copied..]);
            Cow::Owned(quoted)
        }
        None => Cow::Borrowed(text),
    }
}

/// Run `parse` on `text` with wide integers kept as strings. When the
/// rewritten text fails, the original is parsed for the error, so that a
/// syntax error reports the position in the text the caller supplied.
pub fn parse_exact<T, E>(text: &str, mut parse: impl FnMut(&str) -> Result<T, E>) -> Result<T, E> {
    match quote_wide_integers(text) {
        Cow::Borrowed(text) => parse(text),
        Cow::Owned(quoted) => parse(&quoted).map_err(|error| parse(text).err().unwrap_or(error)),
    }
}

/// The index after the string that opens at `open`.
fn string_end(bytes: &[u8], open: usize) -> usize {
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn is_number_byte(byte: u8) -> bool {
    matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
}

fn is_wide_integer(token: &str) -> bool {
    let digits = token.strip_prefix('-').unwrap_or(token);
    !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && token.parse::<i64>().is_err()
        && token.parse::<u64>().is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_integers_outside_the_value_range_are_quoted() {
        let cases = [
            ("18446744073709551616", "\"18446744073709551616\""),
            ("[-9223372036854775809,1]", "[\"-9223372036854775809\",1]"),
            (
                "{\"a\":99999999999999999999}",
                "{\"a\":\"99999999999999999999\"}",
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(quote_wide_integers(text), expected);
        }
    }

    #[test]
    fn representable_numbers_and_string_contents_are_borrowed() {
        for text in [
            "18446744073709551615",
            "-9223372036854775808",
            "99999999999999999999.0",
            "99999999999999999999e0",
            "1E+400",
            "\"99999999999999999999\"",
            "\"a \\\" 99999999999999999999\"",
            "\"\\\\\", 1",
            "\"unterminated 99999999999999999999",
            "",
        ] {
            assert!(
                matches!(quote_wide_integers(text), Cow::Borrowed(_)),
                "{text}"
            );
        }
    }

    #[test]
    fn a_failure_reports_the_position_in_the_original_text() {
        let text = "[99999999999999999999, }";
        let expected = serde_json::from_str::<serde_json::Value>(text).unwrap_err();
        let error =
            parse_exact(text, |text| serde_json::from_str::<serde_json::Value>(text)).unwrap_err();
        assert_eq!(error.to_string(), expected.to_string());
    }
}
