//! The strftime dialect of the `date_format` node, on `jiff` types.
//!
//! `input_format` and `output_format` are public, so the dialect is a
//! contract: every specifier, modifier, accepted input spelling and error
//! text recorded from the earlier `chrono` implementation still holds
//! (IF-231, `tests/test_date_format_golden.rs`). `jiff::fmt::strtime` is a
//! different dialect (it has no `%+`, formats `%f`, `%.f`, `%C`, `%Z`, `%c`,
//! `%x` and `%X` differently, accepts widths and flags this one refuses, and
//! reads two-digit years around another pivot), so the format string is read
//! here and `jiff` supplies the calendar: dates, weekdays, ISO weeks, day of
//! year and arithmetic.
//!
//! Both strings are untrusted. Nothing here panics or allocates beyond the
//! size of its input and output.

mod fields;
mod format;
mod items;
mod parse;
mod resolve;
mod rfc;
mod scan;

use jiff::SignedDuration;
use jiff::civil::DateTime;

pub(super) use fields::Fields;
pub(super) use format::format;
pub(super) use items::items;
pub(super) use parse::parse_prefix;
pub(super) use rfc::{parse_rfc2822, parse_rfc3339};

/// Why an input did not parse. The texts are part of the node's errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ParseError {
    OutOfRange,
    Impossible,
    NotEnough,
    Invalid,
    TooShort,
    TooLong,
    BadFormat,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OutOfRange => "input is out of range",
            Self::Impossible => "no possible date and time matching input",
            Self::NotEnough => "input is not enough for unique date and time",
            Self::Invalid => "input contains invalid characters",
            Self::TooShort => "premature end of input",
            Self::TooLong => "trailing input",
            Self::BadFormat => "bad or unsupported format string",
        })
    }
}

pub(super) type ParseResult<T> = Result<T, ParseError>;

/// A wall-clock time and the offset it is read at.
#[derive(Clone, Copy, Debug)]
pub(super) struct Moment {
    /// The time shown at `offset`.
    pub(super) local: DateTime,
    /// Seconds east of UTC, strictly within one day.
    pub(super) offset: i32,
    /// The value is the leap second after `local`, which reads `:59`.
    pub(super) leap: bool,
}

const UNIX_EPOCH: DateTime = DateTime::constant(1970, 1, 1, 0, 0, 0, 0);

impl Moment {
    /// Unix seconds, rounded down. A leap second counts as the second before.
    pub(super) fn unix_seconds(&self) -> i64 {
        seconds_since_epoch(self.local) - i64::from(self.offset)
    }

    /// The same instant read at another offset, or `None` outside years
    /// -9999 to 9999.
    pub(super) fn at_offset(&self, offset: i32) -> Option<Self> {
        let shift = SignedDuration::from_secs(i64::from(offset) - i64::from(self.offset));
        let local = self.local.checked_add(shift).ok()?;
        Some(Self {
            local,
            offset,
            leap: self.leap,
        })
    }
}

/// Whole seconds from 1970-01-01T00:00:00 to `local`, rounded down.
pub(super) fn seconds_since_epoch(local: DateTime) -> i64 {
    let elapsed = local.duration_since(UNIX_EPOCH);
    // `as_secs` rounds toward zero; a fraction before 1970 belongs to the
    // second below.
    elapsed.as_secs() - i64::from(elapsed.subsec_nanos() < 0)
}

/// The wall-clock time `seconds` after 1970-01-01T00:00:00.
pub(super) fn from_epoch_seconds(seconds: i64) -> Option<DateTime> {
    UNIX_EPOCH
        .checked_add(SignedDuration::from_secs(seconds))
        .ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pad {
    None,
    Zero,
    Space,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Numeric {
    Year,
    YearDiv100,
    YearMod100,
    IsoYear,
    IsoYearMod100,
    Quarter,
    Month,
    Day,
    WeekFromSun,
    WeekFromMon,
    IsoWeek,
    NumDaysFromSun,
    WeekdayFromMon,
    Ordinal,
    Hour,
    Hour12,
    Minute,
    Second,
    Nanosecond,
    Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fixed {
    ShortMonthName,
    LongMonthName,
    ShortWeekdayName,
    LongWeekdayName,
    LowerAmPm,
    UpperAmPm,
    /// `%.f`: nothing, or a dot and 3, 6 or 9 digits.
    Fraction,
    /// `%.3f`, `%.6f`, `%.9f` (with the dot) and `%3f`, `%6f`, `%9f`.
    FractionDigits {
        digits: usize,
        dot: bool,
    },
    /// `%Z`.
    ZoneName,
    /// `%z`, `%:z`, `%::z`, `%:::z` by number of colons.
    Offset {
        colons: u8,
    },
    /// `%#z`: parse only.
    OffsetPermissive,
    /// `%+`.
    Rfc3339,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Item<'a> {
    Literal(&'a str),
    Space(&'a str),
    Numeric(Numeric, Pad),
    Fixed(Fixed),
}
