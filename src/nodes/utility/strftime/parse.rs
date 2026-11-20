//! Reading an input with a list of format items.

use super::scan::{self, OffsetForm, Separator};
use super::{Fields, Fixed, Item, Numeric, ParseError, ParseResult};

/// The offset spelling `%z`, `%:z`, `%::z` and `%:::z` read: all four accept
/// the same input.
const OFFSET: OffsetForm = OffsetForm {
    separator: Separator::ColonsOrSpaces,
    zulu: false,
    missing_minutes: false,
    minus_sign: true,
};

/// Read `input` with `items` and return what the items did not consume.
pub(in crate::nodes::utility) fn parse_prefix<'a>(
    fields: &mut Fields,
    mut input: &'a str,
    items: &[Item<'_>],
) -> ParseResult<&'a str> {
    for item in items {
        input = match *item {
            Item::Literal(literal) => {
                if input.len() < literal.len() {
                    return Err(ParseError::TooShort);
                }
                input.strip_prefix(literal).ok_or(ParseError::Invalid)?
            }
            Item::Space(_) => input.trim_start(),
            Item::Numeric(numeric, _) => parse_numeric(fields, input, numeric)?,
            Item::Fixed(fixed) => parse_fixed(fields, input, fixed)?,
        };
    }
    Ok(input)
}

/// Read a number of at most `width` digits. A signed number may instead
/// carry a `+` or `-` and then any number of digits.
fn number(input: &str, width: usize, signed: bool) -> ParseResult<(&str, i64)> {
    if signed {
        if let Some(digits) = input.strip_prefix('-') {
            let (rest, value) = scan::number(digits, 1, usize::MAX)?;
            return Ok((rest, -value));
        }
        if let Some(digits) = input.strip_prefix('+') {
            return scan::number(digits, 1, usize::MAX);
        }
    }
    scan::number(input, 1, width)
}

fn parse_numeric<'a>(
    fields: &mut Fields,
    input: &'a str,
    numeric: Numeric,
) -> ParseResult<&'a str> {
    type Setter = fn(&mut Fields, i64) -> ParseResult<()>;
    let (width, signed, set): (usize, bool, Setter) = match numeric {
        Numeric::Year => (4, true, Fields::set_year),
        Numeric::YearDiv100 => (2, false, Fields::set_year_div_100),
        Numeric::YearMod100 => (2, false, Fields::set_year_mod_100),
        Numeric::IsoYear => (4, true, Fields::set_iso_year),
        Numeric::IsoYearMod100 => (2, false, Fields::set_iso_year_mod_100),
        Numeric::Quarter => (1, false, Fields::set_quarter),
        Numeric::Month => (2, false, Fields::set_month),
        Numeric::Day => (2, false, Fields::set_day),
        Numeric::WeekFromSun => (2, false, Fields::set_week_from_sun),
        Numeric::WeekFromMon => (2, false, Fields::set_week_from_mon),
        Numeric::IsoWeek => (2, false, Fields::set_iso_week),
        Numeric::NumDaysFromSun => (1, false, Fields::set_weekday_from_sunday),
        Numeric::WeekdayFromMon => (1, false, Fields::set_weekday_from_monday),
        Numeric::Ordinal => (3, false, Fields::set_ordinal),
        Numeric::Hour => (2, false, Fields::set_hour),
        Numeric::Hour12 => (2, false, Fields::set_hour12),
        Numeric::Minute => (2, false, Fields::set_minute),
        Numeric::Second => (2, false, Fields::set_second),
        Numeric::Nanosecond => (9, false, Fields::set_nanosecond),
        Numeric::Timestamp => (usize::MAX, false, Fields::set_timestamp),
    };
    let (rest, value) = number(input.trim_start(), width, signed)?;
    set(fields, value)?;
    Ok(rest)
}

/// Read `AM` or `PM` in any case.
fn am_pm(input: &str) -> ParseResult<(&str, bool)> {
    let pm = match input.as_bytes() {
        [first, second, ..] => match (first | 32, second | 32) {
            (b'a', b'm') => false,
            (b'p', b'm') => true,
            _ => return Err(ParseError::Invalid),
        },
        _ => return Err(ParseError::TooShort),
    };
    Ok((&input[2..], pm))
}

fn month_number(month0: usize) -> i64 {
    i64::try_from(month0).map_or(0, |month0| month0 + 1)
}

fn parse_fixed<'a>(fields: &mut Fields, input: &'a str, fixed: Fixed) -> ParseResult<&'a str> {
    match fixed {
        Fixed::ShortMonthName => {
            let (rest, month0) = scan::short_month0(input)?;
            fields.set_month(month_number(month0))?;
            Ok(rest)
        }
        Fixed::LongMonthName => {
            let (rest, month0) = scan::short_or_long_month0(input)?;
            fields.set_month(month_number(month0))?;
            Ok(rest)
        }
        Fixed::ShortWeekdayName => {
            let (rest, weekday) = scan::short_weekday(input)?;
            fields.set_weekday(weekday)?;
            Ok(rest)
        }
        Fixed::LongWeekdayName => {
            let (rest, weekday) = scan::short_or_long_weekday(input)?;
            fields.set_weekday(weekday)?;
            Ok(rest)
        }
        Fixed::LowerAmPm | Fixed::UpperAmPm => {
            let (rest, pm) = am_pm(input)?;
            fields.set_pm(pm)?;
            Ok(rest)
        }
        Fixed::Fraction => parse_fraction(fields, input, None),
        Fixed::FractionDigits { digits, dot: true } => parse_fraction(fields, input, Some(digits)),
        Fixed::FractionDigits { digits, dot: false } => {
            if input.len() < digits {
                return Err(ParseError::TooShort);
            }
            let (rest, nanos) = scan::nanosecond_fixed(input, digits)?;
            fields.set_nanosecond(nanos)?;
            Ok(rest)
        }
        // The name is skipped, not read: the caller decides what it may be.
        Fixed::ZoneName => Ok(input.trim_start_matches(|c: char| !c.is_whitespace())),
        Fixed::Offset { .. } => parse_offset(fields, input, OFFSET),
        Fixed::OffsetPermissive => {
            let form = OffsetForm {
                zulu: true,
                missing_minutes: true,
                ..OFFSET
            };
            parse_offset(fields, input, form)
        }
        Fixed::Rfc3339 => super::rfc::parse_rfc3339_relaxed(fields, input),
    }
}

/// Read an optional `.` and fraction: of any length, or of exactly `digits`.
fn parse_fraction<'a>(
    fields: &mut Fields,
    input: &'a str,
    digits: Option<usize>,
) -> ParseResult<&'a str> {
    let Some(fraction) = input.strip_prefix('.') else {
        return Ok(input);
    };
    let (rest, nanos) = match digits {
        Some(digits) => scan::nanosecond_fixed(fraction, digits)?,
        None => scan::nanosecond(fraction)?,
    };
    fields.set_nanosecond(nanos)?;
    Ok(rest)
}

fn parse_offset<'a>(fields: &mut Fields, input: &'a str, form: OffsetForm) -> ParseResult<&'a str> {
    let (rest, offset) = scan::timezone_offset(input.trim_start(), form)?;
    fields.set_offset(offset)?;
    Ok(rest)
}
