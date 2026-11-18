//! Writing a [`Moment`] with a list of format items.

use std::fmt::Write as _;

use jiff::civil::{Date, Weekday};

use super::{Fixed, Item, Moment, Numeric, Pad};

const SHORT_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const LONG_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const SHORT_WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const LONG_WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Weeks since the first `start` weekday of the year; days before it are
/// week 0.
pub(super) fn weeks_from(date: Date, start: Weekday) -> i64 {
    (i64::from(date.day_of_year()) - i64::from(date.weekday().since(start)) + 6) / 7
}

/// Format `moment`, or `None` for `%#z`, which only parses.
pub(in crate::nodes::utility) fn format(moment: &Moment, items: &[Item<'_>]) -> Option<String> {
    let mut out = String::new();
    for item in items {
        match *item {
            Item::Literal(text) | Item::Space(text) => out.push_str(text),
            Item::Numeric(numeric, pad) => write_numeric(&mut out, moment, numeric, pad),
            Item::Fixed(fixed) => write_fixed(&mut out, moment, fixed)?,
        }
    }
    Some(out)
}

/// `value` in at least `width` characters. `signed` always writes a sign,
/// which takes one more character.
fn write_number(out: &mut String, width: usize, value: i64, pad: Pad, signed: bool) {
    // Writing to a `String` cannot fail.
    let _ = match (signed, pad) {
        (true, Pad::None) => write!(out, "{value:+}"),
        (true, Pad::Zero) => write!(out, "{value:+0w$}", w = width + 1),
        (true, Pad::Space) => write!(out, "{value:+w$}", w = width + 1),
        (false, Pad::None) => write!(out, "{value}"),
        (false, Pad::Zero) => write!(out, "{value:0width$}"),
        (false, Pad::Space) => write!(out, "{value:width$}"),
    };
}

/// A value of 0 to 99 in two characters.
///
/// Only a century can leave that range, in a year before 0000. It is then
/// written as the earlier implementation wrote it, which is not a number.
fn write_two(out: &mut String, value: i64, pad: Pad) {
    let byte = value.rem_euclid(256);
    let (tens, ones) = (byte / 10, byte % 10);
    match (tens, pad) {
        (0, Pad::None) => {}
        (0, Pad::Space) => out.push(' '),
        _ => out.push(char::from(b'0' + u8::try_from(tens).unwrap_or(0))),
    }
    out.push(char::from(b'0' + u8::try_from(ones).unwrap_or(0)));
}

/// A year: four digits, and a sign outside 0000 to 9999.
fn write_year(out: &mut String, year: i64, pad: Pad) {
    write_number(out, 4, year, pad, !(0..10_000).contains(&year));
}

fn write_numeric(out: &mut String, moment: &Moment, numeric: Numeric, pad: Pad) {
    let date = moment.local.date();
    let time = moment.local.time();
    let year = i64::from(date.year());
    let iso = date.iso_week_date();
    let two = |out: &mut String, value: i64| write_two(out, value, pad);
    let digit =
        |out: &mut String, value: i8| write_number(out, 1, i64::from(value), Pad::None, false);
    match numeric {
        Numeric::Year => write_year(out, year, pad),
        Numeric::YearDiv100 => two(out, year.div_euclid(100)),
        Numeric::YearMod100 => two(out, year.rem_euclid(100)),
        Numeric::IsoYear => write_year(out, i64::from(iso.year()), pad),
        Numeric::IsoYearMod100 => two(out, i64::from(iso.year()).rem_euclid(100)),
        Numeric::Quarter => digit(out, (date.month() - 1) / 3 + 1),
        Numeric::Month => two(out, i64::from(date.month())),
        Numeric::Day => two(out, i64::from(date.day())),
        Numeric::WeekFromSun => two(out, weeks_from(date, Weekday::Sunday)),
        Numeric::WeekFromMon => two(out, weeks_from(date, Weekday::Monday)),
        Numeric::IsoWeek => two(out, i64::from(iso.week())),
        Numeric::NumDaysFromSun => digit(out, date.weekday().to_sunday_zero_offset()),
        Numeric::WeekdayFromMon => digit(out, date.weekday().to_monday_one_offset()),
        Numeric::Ordinal => write_number(out, 3, i64::from(date.day_of_year()), pad, false),
        Numeric::Hour => two(out, i64::from(time.hour())),
        Numeric::Hour12 => two(out, i64::from((time.hour() + 11) % 12 + 1)),
        Numeric::Minute => two(out, i64::from(time.minute())),
        Numeric::Second => two(out, i64::from(time.second()) + i64::from(moment.leap)),
        Numeric::Nanosecond => {
            write_number(out, 9, i64::from(time.subsec_nanosecond()), pad, false);
        }
        Numeric::Timestamp => write_number(out, 9, moment.unix_seconds(), pad, false),
    }
}

/// `.` and the shortest of 3, 6 or 9 digits that is exact; nothing for zero.
fn write_fraction(out: &mut String, nanos: i32) {
    let _ = if nanos == 0 {
        Ok(())
    } else if nanos % 1_000_000 == 0 {
        write!(out, ".{:03}", nanos / 1_000_000)
    } else if nanos % 1_000 == 0 {
        write!(out, ".{:06}", nanos / 1_000)
    } else {
        write!(out, ".{nanos:09}")
    };
}

/// The offset as `+HHMM`, `+HH:MM`, `+HH:MM:SS` or `+HH` by number of
/// colons. Offsets are whole minutes, so no form rounds.
fn write_offset(out: &mut String, offset: i32, colons: u8) {
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.unsigned_abs() / 60;
    let (hours, minutes) = (minutes / 60, minutes % 60);
    let _ = match colons {
        0 => write!(out, "{sign}{hours:02}{minutes:02}"),
        1 => write!(out, "{sign}{hours:02}:{minutes:02}"),
        2 => write!(out, "{sign}{hours:02}:{minutes:02}:00"),
        _ => write!(out, "{sign}{hours:02}"),
    };
}

fn write_rfc3339(out: &mut String, moment: &Moment) {
    let (date, time) = (moment.local.date(), moment.local.time());
    let year = i64::from(date.year());
    let _ = if (0..=9999).contains(&year) {
        write!(out, "{year:04}")
    } else {
        write!(out, "{year:+05}")
    };
    let _ = write!(
        out,
        "-{:02}-{:02}T{:02}:{:02}:{:02}",
        date.month(),
        date.day(),
        time.hour(),
        time.minute(),
        i64::from(time.second()) + i64::from(moment.leap),
    );
    write_fraction(out, time.subsec_nanosecond());
    write_offset(out, moment.offset, 1);
}

fn write_fixed(out: &mut String, moment: &Moment, fixed: Fixed) -> Option<()> {
    let date = moment.local.date();
    let time = moment.local.time();
    let nanos = time.subsec_nanosecond();
    let month = usize::try_from(date.month() - 1).ok()?;
    let weekday = usize::try_from(date.weekday().to_sunday_zero_offset()).ok()?;
    match fixed {
        Fixed::ShortMonthName => out.push_str(SHORT_MONTHS[month]),
        Fixed::LongMonthName => out.push_str(LONG_MONTHS[month]),
        Fixed::ShortWeekdayName => out.push_str(SHORT_WEEKDAYS[weekday]),
        Fixed::LongWeekdayName => out.push_str(LONG_WEEKDAYS[weekday]),
        Fixed::LowerAmPm => out.push_str(if time.hour() < 12 { "am" } else { "pm" }),
        Fixed::UpperAmPm => out.push_str(if time.hour() < 12 { "AM" } else { "PM" }),
        Fixed::Fraction => write_fraction(out, nanos),
        Fixed::FractionDigits { digits, dot } => {
            if dot {
                out.push('.');
            }
            let shown = i64::from(nanos) / 10_i64.pow(9 - u32::try_from(digits).ok()?);
            write_number(out, digits, shown, Pad::Zero, false);
        }
        // `%Z` writes the offset: no zone name is known.
        Fixed::ZoneName => write_offset(out, moment.offset, 1),
        Fixed::Offset { colons } => write_offset(out, moment.offset, colons),
        Fixed::OffsetPermissive => return None,
        Fixed::Rfc3339 => write_rfc3339(out, moment),
    }
    Some(())
}
