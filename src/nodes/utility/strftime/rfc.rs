//! The RFC 3339 and RFC 2822 readers the node has always used: stricter than
//! `jiff`'s in some places (no ISO 8601 basic form, no bracketed zone) and
//! more lenient in others (`UTC` as an offset, two-digit and three-digit
//! RFC 2822 years, the obsolete zone names).

use jiff::civil::{Date, DateTime, Time};

use super::scan::{self, OffsetForm, Separator};
use super::{Fields, Item, Moment, Numeric, Pad, ParseError, ParseResult, parse_prefix};

/// Offsets within a day of UTC are the only ones a value may carry.
fn in_one_day(offset: i32) -> ParseResult<i32> {
    if (-86_399..=86_399).contains(&offset) {
        Ok(offset)
    } else {
        Err(ParseError::OutOfRange)
    }
}

fn digit(bytes: &[u8], index: usize) -> ParseResult<u8> {
    match bytes.get(index) {
        Some(byte) if byte.is_ascii_digit() => Ok(byte - b'0'),
        _ => Err(ParseError::Invalid),
    }
}

fn two_digits(bytes: &[u8], index: usize) -> ParseResult<i8> {
    let value = digit(bytes, index)? * 10 + digit(bytes, index + 1)?;
    i8::try_from(value).map_err(|_| ParseError::OutOfRange)
}

fn expect(bytes: &[u8], index: usize, accepted: &[u8]) -> ParseResult<()> {
    match bytes.get(index) {
        Some(byte) if accepted.contains(byte) => Ok(()),
        _ => Err(ParseError::Invalid),
    }
}

/// Read a whole input as strict RFC 3339: `YYYY-MM-DDTHH:MM:SS`, an optional
/// fraction, then `Z` or `+HH:MM`. `T` and `Z` may be lower case and a space
/// may stand for `T`.
pub(in crate::nodes::utility) fn parse_rfc3339(input: &str) -> ParseResult<Moment> {
    let bytes = input.as_bytes();
    if bytes.len() < 19 {
        return Err(ParseError::TooShort);
    }
    let year = i16::from(digit(bytes, 0)?) * 1000
        + i16::from(digit(bytes, 1)?) * 100
        + i16::from(digit(bytes, 2)?) * 10
        + i16::from(digit(bytes, 3)?);
    expect(bytes, 4, b"-")?;
    let month = two_digits(bytes, 5)?;
    expect(bytes, 7, b"-")?;
    let day = two_digits(bytes, 8)?;
    let date = Date::new(year, month, day).map_err(|_| ParseError::OutOfRange)?;
    expect(bytes, 10, b"tT ")?;
    let hour = two_digits(bytes, 11)?;
    expect(bytes, 13, b":")?;
    let minute = two_digits(bytes, 14)?;
    expect(bytes, 16, b":")?;
    let (second, leap) = match two_digits(bytes, 17)? {
        60 => (59, true),
        second => (second, false),
    };

    // The first nineteen bytes are ASCII, so these are character boundaries.
    let (rest, nanos) = match bytes.get(19) {
        Some(b'.') => scan::nanosecond(&input[20..])?,
        _ => (&input[19..], 0),
    };
    let nanos = i32::try_from(nanos).map_err(|_| ParseError::OutOfRange)?;
    let time = Time::new(hour, minute, second, nanos).map_err(|_| ParseError::OutOfRange)?;

    let form = OffsetForm {
        separator: Separator::Colon,
        zulu: true,
        missing_minutes: false,
        minus_sign: true,
    };
    let (rest, offset) = scan::timezone_offset(rest, form)?;
    if !rest.is_empty() {
        return Err(ParseError::TooLong);
    }
    Ok(Moment {
        local: DateTime::from_parts(date, time),
        offset: in_one_day(offset)?,
        leap,
    })
}

const fn zero(numeric: Numeric) -> Item<'static> {
    Item::Numeric(numeric, Pad::Zero)
}

/// `%+`: RFC 3339 read field by field, so numbers may be short or spaced,
/// the offset may be `UTC`, and its colon may be missing.
pub(super) fn parse_rfc3339_relaxed<'a>(
    fields: &mut Fields,
    input: &'a str,
) -> ParseResult<&'a str> {
    const DATE: &[Item<'static>] = &[
        zero(Numeric::Year),
        Item::Space(""),
        Item::Literal("-"),
        zero(Numeric::Month),
        Item::Space(""),
        Item::Literal("-"),
        zero(Numeric::Day),
    ];
    const TIME: &[Item<'static>] = &[
        zero(Numeric::Hour),
        Item::Space(""),
        Item::Literal(":"),
        zero(Numeric::Minute),
        Item::Space(""),
        Item::Literal(":"),
        zero(Numeric::Second),
        Item::Fixed(super::Fixed::Fraction),
        Item::Space(""),
    ];
    let rest = parse_prefix(fields, input, DATE)?;
    let rest = match rest.as_bytes().first() {
        Some(b't' | b'T' | b' ') => &rest[1..],
        Some(_) => return Err(ParseError::Invalid),
        None => return Err(ParseError::TooShort),
    };
    let rest = parse_prefix(fields, rest, TIME)?.trim_start();
    let named_utc = rest
        .as_bytes()
        .get(..3)
        .is_some_and(|front| front.eq_ignore_ascii_case(b"UTC"));
    let (rest, offset) = if named_utc {
        (&rest[3..], 0)
    } else {
        let form = OffsetForm {
            separator: Separator::ColonsOrSpaces,
            zulu: true,
            missing_minutes: false,
            minus_sign: true,
        };
        scan::timezone_offset(rest, form)?
    };
    fields.set_offset(offset)?;
    Ok(rest)
}

/// Scan the front of `input` and move it past what was read.
fn take<'a, T>(
    input: &mut &'a str,
    scan: impl FnOnce(&'a str) -> ParseResult<(&'a str, T)>,
) -> ParseResult<T> {
    let (rest, value) = scan(input)?;
    *input = rest;
    Ok(value)
}

/// A two-digit year is 2000 to 2049 or 1950 to 1999; three digits count from
/// 1900.
fn rfc2822_year(year: i64, digits: usize) -> i64 {
    match (digits, year) {
        (2, 0..=49) => year + 2000,
        (2, _) | (3, _) => year + 1900,
        _ => year,
    }
}

fn read_rfc2822<'a>(fields: &mut Fields, input: &'a str) -> ParseResult<&'a str> {
    let mut s = input.trim_start();
    if let Ok((rest, weekday)) = scan::short_weekday(s) {
        s = rest.strip_prefix(',').ok_or(ParseError::Invalid)?;
        fields.set_weekday(weekday)?;
    }
    s = s.trim_start();
    fields.set_day(take(&mut s, |s| scan::number(s, 1, 2))?)?;
    s = scan::space(s)?;
    let month0 = take(&mut s, scan::short_month0)?;
    fields.set_month(i64::try_from(month0).map_or(0, |month0| month0 + 1))?;
    s = scan::space(s)?;

    let before = s.len();
    let year = take(&mut s, |s| scan::number(s, 2, usize::MAX))?;
    fields.set_year(rfc2822_year(year, before - s.len()))?;

    s = scan::space(s)?;
    fields.set_hour(take(&mut s, |s| scan::number(s, 2, 2))?)?;
    s = scan::byte(s.trim_start(), b':')?.trim_start();
    fields.set_minute(take(&mut s, |s| scan::number(s, 2, 2))?)?;
    if let Ok(seconds) = scan::byte(s.trim_start(), b':') {
        s = seconds;
        fields.set_second(take(&mut s, |s| scan::number(s, 2, 2))?)?;
    }

    s = scan::space(s)?;
    fields.set_offset(take(&mut s, scan::timezone_offset_2822)?)?;
    while let Ok(rest) = scan::comment_2822(s) {
        s = rest;
    }
    Ok(s)
}

/// Read a whole input as an RFC 2822 date.
pub(in crate::nodes::utility) fn parse_rfc2822(input: &str) -> ParseResult<Moment> {
    let mut fields = Fields::default();
    if !read_rfc2822(&mut fields, input)?.is_empty() {
        return Err(ParseError::TooLong);
    }
    let offset = fields.offset().ok_or(ParseError::NotEnough)?;
    let resolved = fields.date_time(offset)?;
    Ok(Moment {
        local: resolved.local,
        offset: in_one_day(offset)?,
        leap: resolved.leap,
    })
}
