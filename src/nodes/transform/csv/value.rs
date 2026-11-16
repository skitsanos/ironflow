use anyhow::Result;
use serde_json::Value;

pub(super) fn parse_csv_single_byte(config: &Value, key: &str, default: u8) -> Result<u8> {
    let Some(value) = config.get(key).and_then(|value| value.as_str()) else {
        return Ok(default);
    };
    match value {
        "\\t" => return Ok(b'\t'),
        "\\n" => return Ok(b'\n'),
        "\\r" => return Ok(b'\r'),
        _ => {}
    }
    let bytes = value.as_bytes();
    if bytes.len() != 1 {
        anyhow::bail!("{} must be a single-byte character", key);
    }
    Ok(bytes[0])
}

pub(super) fn csv_value_from_str(value: &str, infer_types: bool) -> Value {
    if !infer_types {
        return Value::String(value.to_string());
    }
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    inferred_number(trimmed).unwrap_or_else(|| Value::String(trimmed.to_string()))
}

/// A float carries 15 significant decimal digits without loss; a field with
/// more stays text so that identifiers and large integers keep every digit.
const MAX_SIGNIFICANT_DIGITS: usize = 15;

/// Converts a field only when it is written as a plain decimal number:
/// `[-]digits[.digits][e[+-]digits]`, where the digits on one side of the
/// decimal point may be omitted. A leading `+`, a leading zero before another
/// digit, `-0`, more than 15 significant digits, a value outside the float
/// range and the words `nan`, `inf` and `infinity` all stay text (IF-196).
fn inferred_number(field: &str) -> Option<Value> {
    let all_digits = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
    let unsigned = field.strip_prefix('-').unwrap_or(field);
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (unsigned, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    let fraction_digits = fraction.unwrap_or_default();
    if !all_digits(whole) || !all_digits(fraction_digits) {
        return None;
    }
    if whole.is_empty() && fraction_digits.is_empty() {
        return None;
    }
    if whole.len() > 1 && whole.starts_with('0') {
        return None;
    }
    if let Some(exponent) = exponent {
        let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        if digits.is_empty() || !all_digits(digits) {
            return None;
        }
    }
    let significant = whole
        .bytes()
        .chain(fraction_digits.bytes())
        .skip_while(|byte| *byte == b'0')
        .count();
    if significant > MAX_SIGNIFICANT_DIGITS {
        return None;
    }
    if fraction.is_none() && exponent.is_none() {
        if field == "-0" {
            return None;
        }
        return field.parse::<i64>().ok().map(Value::from);
    }
    let number = field.parse::<f64>().ok()?;
    serde_json::Number::from_f64(number).map(Value::Number)
}

pub(super) fn csv_value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}
