//! The fields read from an input. A field may be given more than once and
//! by more than one specifier; a repeated field must agree with itself.
//! `resolve.rs` turns the fields into one date and time.

use jiff::civil::Weekday;

use super::{ParseError, ParseResult};

/// Everything a format string can read. `None` is "not given".
#[derive(Clone, Debug, Default)]
pub(in crate::nodes::utility) struct Fields {
    pub(super) year: Option<i32>,
    pub(super) year_div_100: Option<i32>,
    pub(super) year_mod_100: Option<i32>,
    pub(super) iso_year: Option<i32>,
    pub(super) iso_year_mod_100: Option<i32>,
    pub(super) quarter: Option<i64>,
    pub(super) month: Option<i64>,
    pub(super) week_from_sun: Option<i64>,
    pub(super) week_from_mon: Option<i64>,
    pub(super) iso_week: Option<i64>,
    pub(super) weekday: Option<Weekday>,
    pub(super) ordinal: Option<i64>,
    pub(super) day: Option<i64>,
    /// 0 before noon, 1 after.
    pub(super) hour_div_12: Option<i64>,
    pub(super) hour_mod_12: Option<i64>,
    pub(super) minute: Option<i64>,
    /// 0 to 60; 60 is a leap second.
    pub(super) second: Option<i64>,
    pub(super) nanosecond: Option<i64>,
    pub(super) timestamp: Option<i64>,
    pub(super) offset: Option<i32>,
}

fn set<T: PartialEq>(slot: &mut Option<T>, value: T) -> ParseResult<()> {
    match slot {
        Some(held) if *held != value => Err(ParseError::Impossible),
        _ => {
            *slot = Some(value);
            Ok(())
        }
    }
}

fn set_in(
    slot: &mut Option<i64>,
    value: i64,
    range: std::ops::RangeInclusive<i64>,
) -> ParseResult<()> {
    if !range.contains(&value) {
        return Err(ParseError::OutOfRange);
    }
    set(slot, value)
}

fn as_i32(value: i64) -> ParseResult<i32> {
    i32::try_from(value).map_err(|_| ParseError::OutOfRange)
}

impl Fields {
    pub(super) fn set_year(&mut self, value: i64) -> ParseResult<()> {
        set(&mut self.year, as_i32(value)?)
    }

    pub(super) fn set_year_div_100(&mut self, value: i64) -> ParseResult<()> {
        if value < 0 {
            return Err(ParseError::OutOfRange);
        }
        set(&mut self.year_div_100, as_i32(value)?)
    }

    pub(super) fn set_year_mod_100(&mut self, value: i64) -> ParseResult<()> {
        if !(0..100).contains(&value) {
            return Err(ParseError::OutOfRange);
        }
        set(&mut self.year_mod_100, as_i32(value)?)
    }

    pub(super) fn set_iso_year(&mut self, value: i64) -> ParseResult<()> {
        set(&mut self.iso_year, as_i32(value)?)
    }

    pub(super) fn set_iso_year_mod_100(&mut self, value: i64) -> ParseResult<()> {
        if !(0..100).contains(&value) {
            return Err(ParseError::OutOfRange);
        }
        set(&mut self.iso_year_mod_100, as_i32(value)?)
    }

    pub(super) fn set_quarter(&mut self, value: i64) -> ParseResult<()> {
        set_in(&mut self.quarter, value, 1..=4)
    }

    pub(super) fn set_month(&mut self, value: i64) -> ParseResult<()> {
        set_in(&mut self.month, value, 1..=12)
    }

    pub(super) fn set_week_from_sun(&mut self, value: i64) -> ParseResult<()> {
        set_in(&mut self.week_from_sun, value, 0..=53)
    }

    pub(super) fn set_week_from_mon(&mut self, value: i64) -> ParseResult<()> {
        set_in(&mut self.week_from_mon, value, 0..=53)
    }

    pub(super) fn set_iso_week(&mut self, value: i64) -> ParseResult<()> {
        set_in(&mut self.iso_week, value, 1..=53)
    }

    pub(super) fn set_weekday(&mut self, value: Weekday) -> ParseResult<()> {
        set(&mut self.weekday, value)
    }

    /// `%w`: Sunday is 0.
    pub(super) fn set_weekday_from_sunday(&mut self, value: i64) -> ParseResult<()> {
        let weekday = i8::try_from(value)
            .ok()
            .and_then(|value| Weekday::from_sunday_zero_offset(value).ok());
        self.set_weekday(weekday.ok_or(ParseError::OutOfRange)?)
    }

    /// `%u`: Monday is 1.
    pub(super) fn set_weekday_from_monday(&mut self, value: i64) -> ParseResult<()> {
        let weekday = i8::try_from(value)
            .ok()
            .and_then(|value| Weekday::from_monday_one_offset(value).ok());
        self.set_weekday(weekday.ok_or(ParseError::OutOfRange)?)
    }

    pub(super) fn set_ordinal(&mut self, value: i64) -> ParseResult<()> {
        set_in(&mut self.ordinal, value, 1..=366)
    }

    pub(super) fn set_day(&mut self, value: i64) -> ParseResult<()> {
        set_in(&mut self.day, value, 1..=31)
    }

    pub(super) fn set_pm(&mut self, pm: bool) -> ParseResult<()> {
        set(&mut self.hour_div_12, i64::from(pm))
    }

    pub(super) fn set_hour12(&mut self, value: i64) -> ParseResult<()> {
        if !(1..=12).contains(&value) {
            return Err(ParseError::OutOfRange);
        }
        set(&mut self.hour_mod_12, value % 12)
    }

    pub(super) fn set_hour(&mut self, value: i64) -> ParseResult<()> {
        if !(0..=23).contains(&value) {
            return Err(ParseError::OutOfRange);
        }
        set(&mut self.hour_div_12, value / 12)?;
        set(&mut self.hour_mod_12, value % 12)
    }

    pub(super) fn set_minute(&mut self, value: i64) -> ParseResult<()> {
        set_in(&mut self.minute, value, 0..=59)
    }

    pub(super) fn set_second(&mut self, value: i64) -> ParseResult<()> {
        set_in(&mut self.second, value, 0..=60)
    }

    pub(super) fn set_nanosecond(&mut self, value: i64) -> ParseResult<()> {
        set_in(&mut self.nanosecond, value, 0..=999_999_999)
    }

    pub(super) fn set_timestamp(&mut self, value: i64) -> ParseResult<()> {
        set(&mut self.timestamp, value)
    }

    pub(super) fn set_offset(&mut self, seconds: i32) -> ParseResult<()> {
        set(&mut self.offset, seconds)
    }

    /// The offset read, in seconds east of UTC.
    pub(in crate::nodes::utility) fn offset(&self) -> Option<i32> {
        self.offset
    }
}
