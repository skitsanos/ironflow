//! Scanners for the pieces of a date: each takes the front of the input and
//! returns what is left with the value read.

use jiff::civil::Weekday;

use super::{ParseError, ParseResult};

/// Read `min` to `max` ASCII digits.
pub(super) fn number(s: &str, min: usize, max: usize) -> ParseResult<(&str, i64)> {
    let bytes = s.as_bytes();
    if bytes.len() < min {
        return Err(ParseError::TooShort);
    }
    let mut value = 0_i64;
    for (index, byte) in bytes.iter().take(max).enumerate() {
        if !byte.is_ascii_digit() {
            return if index < min {
                Err(ParseError::Invalid)
            } else {
                Ok((&s[index..], value))
            };
        }
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(i64::from(byte - b'0')))
            .ok_or(ParseError::OutOfRange)?;
    }
    Ok((&s[max.min(bytes.len())..], value))
}

const SCALE: [i64; 10] = [
    0,
    100_000_000,
    10_000_000,
    1_000_000,
    100_000,
    10_000,
    1_000,
    100,
    10,
    1,
];

/// Read a fraction of a second of any length as nanoseconds; digits past the
/// ninth are dropped.
pub(super) fn nanosecond(s: &str) -> ParseResult<(&str, i64)> {
    let (rest, value) = number(s, 1, 9)?;
    let value = value * SCALE[s.len() - rest.len()];
    Ok((rest.trim_start_matches(|c: char| c.is_ascii_digit()), value))
}

/// Read exactly `digits` digits of a fraction as nanoseconds.
pub(super) fn nanosecond_fixed(s: &str, digits: usize) -> ParseResult<(&str, i64)> {
    let (rest, value) = number(s, digits, digits)?;
    Ok((rest, value * SCALE[digits]))
}

/// The first three bytes, lower-cased, when there are three.
fn three_letters(s: &str) -> ParseResult<[u8; 3]> {
    match s.as_bytes() {
        [a, b, c, ..] => Ok([a | 32, b | 32, c | 32]),
        _ => Err(ParseError::TooShort),
    }
}

/// Read a three-letter month name, any case, as 0 to 11.
pub(super) fn short_month0(s: &str) -> ParseResult<(&str, usize)> {
    const NAMES: [&[u8; 3]; 12] = [
        b"jan", b"feb", b"mar", b"apr", b"may", b"jun", b"jul", b"aug", b"sep", b"oct", b"nov",
        b"dec",
    ];
    let letters = three_letters(s)?;
    let month0 = NAMES
        .iter()
        .position(|name| **name == letters)
        .ok_or(ParseError::Invalid)?;
    Ok((&s[3..], month0))
}

/// Read a three-letter weekday name, any case.
pub(super) fn short_weekday(s: &str) -> ParseResult<(&str, Weekday)> {
    let weekday = match &three_letters(s)? {
        b"mon" => Weekday::Monday,
        b"tue" => Weekday::Tuesday,
        b"wed" => Weekday::Wednesday,
        b"thu" => Weekday::Thursday,
        b"fri" => Weekday::Friday,
        b"sat" => Weekday::Saturday,
        b"sun" => Weekday::Sunday,
        _ => return Err(ParseError::Invalid),
    };
    Ok((&s[3..], weekday))
}

/// Drop `suffix` from the front of `s` when it is there, in any case.
fn skip_suffix<'a>(s: &'a str, suffix: &str) -> &'a str {
    match s.as_bytes().get(..suffix.len()) {
        Some(front) if front.eq_ignore_ascii_case(suffix.as_bytes()) => &s[suffix.len()..],
        _ => s,
    }
}

/// Read a month name, full or abbreviated.
pub(super) fn short_or_long_month0(s: &str) -> ParseResult<(&str, usize)> {
    const SUFFIXES: [&str; 12] = [
        "uary", "ruary", "ch", "il", "", "e", "y", "ust", "tember", "ober", "ember", "ember",
    ];
    let (rest, month0) = short_month0(s)?;
    Ok((skip_suffix(rest, SUFFIXES[month0]), month0))
}

/// Read a weekday name, full or abbreviated.
pub(super) fn short_or_long_weekday(s: &str) -> ParseResult<(&str, Weekday)> {
    let (rest, weekday) = short_weekday(s)?;
    let suffix = match weekday {
        Weekday::Monday | Weekday::Friday | Weekday::Sunday => "day",
        Weekday::Tuesday => "sday",
        Weekday::Wednesday => "nesday",
        Weekday::Thursday => "rsday",
        Weekday::Saturday => "urday",
    };
    Ok((skip_suffix(rest, suffix), weekday))
}

/// Read one given byte.
pub(super) fn byte(s: &str, expected: u8) -> ParseResult<&str> {
    match s.as_bytes().first() {
        Some(&found) if found == expected => Ok(&s[1..]),
        Some(_) => Err(ParseError::Invalid),
        None => Err(ParseError::TooShort),
    }
}

/// Read at least one whitespace character.
pub(super) fn space(s: &str) -> ParseResult<&str> {
    let rest = s.trim_start();
    if rest.len() < s.len() {
        Ok(rest)
    } else if s.is_empty() {
        Err(ParseError::TooShort)
    } else {
        Err(ParseError::Invalid)
    }
}

/// What may stand between the hours and minutes of an offset.
#[derive(Clone, Copy)]
pub(super) enum Separator {
    /// Nothing (RFC 2822).
    Nothing,
    /// Exactly one colon (strict RFC 3339).
    Colon,
    /// Any run of colons and whitespace, or nothing.
    ColonsOrSpaces,
}

/// How an offset may be spelled.
#[derive(Clone, Copy)]
pub(super) struct OffsetForm {
    pub(super) separator: Separator,
    /// `Z` or `z` stands for UTC.
    pub(super) zulu: bool,
    /// The minutes may be left out.
    pub(super) missing_minutes: bool,
    /// U+2212 MINUS SIGN is accepted as the sign.
    pub(super) minus_sign: bool,
}

fn two_bytes(s: &str) -> Option<(u8, u8)> {
    match s.as_bytes() {
        [a, b, ..] => Some((*a, *b)),
        _ => None,
    }
}

/// Read an offset as seconds east of UTC.
pub(super) fn timezone_offset(mut s: &str, form: OffsetForm) -> ParseResult<(&str, i32)> {
    if form.zulu && matches!(s.as_bytes().first(), Some(b'Z' | b'z')) {
        return Ok((&s[1..], 0));
    }
    let negative = match s.chars().next() {
        Some('+') => false,
        Some('-') => true,
        Some('\u{2212}') if form.minus_sign => true,
        Some(_) => return Err(ParseError::Invalid),
        None => return Err(ParseError::TooShort),
    };
    s = &s[s.chars().next().map_or(0, char::len_utf8)..];

    let hours = match two_bytes(s).ok_or(ParseError::TooShort)? {
        (h1 @ b'0'..=b'9', h2 @ b'0'..=b'9') => i32::from((h1 - b'0') * 10 + (h2 - b'0')),
        _ => return Err(ParseError::Invalid),
    };
    s = &s[2..];
    s = match form.separator {
        Separator::Nothing => s,
        Separator::Colon => byte(s, b':')?,
        Separator::ColonsOrSpaces => s.trim_start_matches(|c: char| c == ':' || c.is_whitespace()),
    };

    let minutes = match two_bytes(s) {
        Some((m1 @ b'0'..=b'5', m2 @ b'0'..=b'9')) => i32::from((m1 - b'0') * 10 + (m2 - b'0')),
        Some((b'6'..=b'9', b'0'..=b'9')) => return Err(ParseError::OutOfRange),
        Some(_) => return Err(ParseError::Invalid),
        None if form.missing_minutes => 0,
        None => return Err(ParseError::TooShort),
    };
    s = match s.len() {
        0 => s,
        1 => return Err(ParseError::TooShort),
        _ => s.get(2..).ok_or(ParseError::Invalid)?,
    };

    let seconds = hours * 3600 + minutes * 60;
    Ok((s, if negative { -seconds } else { seconds }))
}

/// Read an RFC 2822 zone: a numeric offset or one of the obsolete names.
pub(super) fn timezone_offset_2822(s: &str) -> ParseResult<(&str, i32)> {
    let end = s
        .bytes()
        .position(|byte| !byte.is_ascii_alphabetic())
        .unwrap_or(s.len());
    if end == 0 {
        let form = OffsetForm {
            separator: Separator::Nothing,
            zulu: false,
            missing_minutes: false,
            minus_sign: false,
        };
        return timezone_offset(s, form);
    }
    let (name, rest) = s.split_at(end);
    let hours = match name.to_ascii_lowercase().as_str() {
        "gmt" | "ut" | "z" => 0,
        "edt" => -4,
        "est" | "cdt" => -5,
        "cst" | "mdt" => -6,
        "mst" | "pdt" => -7,
        "pst" => -8,
        // The military zones are read as UTC; `J` is not one of them.
        other if other.len() == 1 && other != "j" => 0,
        _ => return Err(ParseError::Invalid),
    };
    Ok((rest, hours * 3600))
}

/// Read one RFC 2822 comment: nested parentheses with backslash escapes.
pub(super) fn comment_2822(s: &str) -> ParseResult<&str> {
    let s = s.trim_start();
    let mut depth = 0_usize;
    let mut escaped = false;
    for (index, byte) in s.bytes().enumerate() {
        if index == 0 {
            if byte != b'(' {
                return Err(ParseError::Invalid);
            }
            depth = 1;
        } else if escaped {
            escaped = false;
        } else {
            match byte {
                b'\\' => escaped = true,
                b'(' => depth += 1,
                b')' if depth == 1 => return Ok(&s[index + 1..]),
                b')' => depth -= 1,
                _ => {}
            }
        }
    }
    Err(ParseError::TooShort)
}
