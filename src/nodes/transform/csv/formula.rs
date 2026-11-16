//! Cells that a spreadsheet would run as a formula (IF-223).
//!
//! A spreadsheet treats a cell that begins with `=`, `+`, `-` or `@` as a
//! formula, and some strip a leading tab, carriage return or line feed
//! first. Text from an untrusted source that is exported and opened there
//! can then call functions or other programs. Following the OWASP CSV
//! injection guidance, such a cell is written with a leading apostrophe,
//! which makes the spreadsheet read it as text.

use std::borrow::Cow;

use serde_json::Value;

use super::value::csv_value_to_string;

/// Characters that start a formula, including the full-width forms that
/// spreadsheets normalize to the ASCII ones.
const TRIGGERS: [char; 11] = ['=', '+', '-', '@', '\t', '\r', '\n', '＝', '＋', '－', '＠'];

/// How `csv_stringify` turns values and header names into cells.
#[derive(Clone, Copy)]
pub(super) struct Cells {
    pub(super) escape_formulas: bool,
}

impl Cells {
    /// A header name or other text cell.
    pub(super) fn text<'text>(self, text: &'text str) -> Cow<'text, str> {
        if self.escape_formulas && starts_formula(text) {
            Cow::Owned(format!("'{text}"))
        } else {
            Cow::Borrowed(text)
        }
    }

    /// A value cell. Only strings are text: a number or boolean is written
    /// by its type, so `-5` stays `-5`, and a nested value starts with a
    /// bracket.
    pub(super) fn value(self, value: &Value) -> String {
        match value {
            Value::String(text) => self.text(text).into_owned(),
            other => csv_value_to_string(other),
        }
    }
}

/// Text that begins with a trigger, unless all of it is one decimal number:
/// `-5`, `+1.5e3` and `-.25` cannot run anything and are common data.
fn starts_formula(text: &str) -> bool {
    text.starts_with(TRIGGERS) && !is_decimal_number(text)
}

/// `[+-]digits[.digits][e[+-]digits]` with digits on at least one side of
/// the decimal point.
fn is_decimal_number(text: &str) -> bool {
    let all_digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (unsigned, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if !all_digits(whole) || !all_digits(fraction) || (whole.is_empty() && fraction.is_empty()) {
        return false;
    }
    exponent.is_none_or(|exponent| {
        let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        !digits.is_empty() && all_digits(digits)
    })
}
