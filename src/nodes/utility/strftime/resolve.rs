//! The one date and time a set of fields names.
//!
//! The rules are those the node has always applied: the first sufficient
//! group of fields decides the date (year, month and day; then year and day
//! of year; then a week number and weekday; then an ISO week date), and
//! every other field given must be true of that date.

use jiff::civil::{Date, DateTime, ISOWeekDate, Time, Weekday};

use super::format::weeks_from;
use super::{Fields, ParseError, ParseResult, from_epoch_seconds, seconds_since_epoch};

/// A wall-clock time read from an input. `leap` marks second 60, which is
/// held as the second before it.
#[derive(Clone, Copy, Debug)]
pub(in crate::nodes::utility) struct Resolved {
    pub(in crate::nodes::utility) local: DateTime,
    pub(in crate::nodes::utility) leap: bool,
}

fn as_i32(value: i64) -> ParseResult<i32> {
    i32::try_from(value).map_err(|_| ParseError::OutOfRange)
}

impl Fields {
    /// The date the fields name.
    pub(in crate::nodes::utility) fn date(&self) -> ParseResult<Date> {
        let year = resolve_year(self.year, self.year_div_100, self.year_mod_100)?;
        let iso_year = resolve_year(self.iso_year, None, self.iso_year_mod_100)?;
        let (date, check_calendar) = match (year, iso_year) {
            (Some(year), _) if self.month.is_some() && self.day.is_some() => {
                (ymd(year, self.month, self.day)?, false)
            }
            (Some(year), _) if self.ordinal.is_some() => (year_day(year, self.ordinal)?, true),
            (Some(year), _) if self.week_from_sun.is_some() && self.weekday.is_some() => (
                week_date(year, self.week_from_sun, self.weekday, Weekday::Sunday)?,
                true,
            ),
            (Some(year), _) if self.week_from_mon.is_some() && self.weekday.is_some() => (
                week_date(year, self.week_from_mon, self.weekday, Weekday::Monday)?,
                true,
            ),
            (_, Some(iso_year)) if self.iso_week.is_some() && self.weekday.is_some() => {
                (iso_week_date(iso_year, self.iso_week, self.weekday)?, true)
            }
            _ => return Err(ParseError::NotEnough),
        };
        let agrees = (!check_calendar || self.agrees_with_calendar(date))
            && self.agrees_with_iso_week(date)
            && self.agrees_with_day_of_year(date)
            && self
                .quarter
                .is_none_or(|quarter| quarter == i64::from((date.month() - 1) / 3 + 1));
        if agrees {
            Ok(date)
        } else {
            Err(ParseError::Impossible)
        }
    }

    fn agrees_with_calendar(&self, date: Date) -> bool {
        let year = i32::from(date.year());
        // A century and a two-digit year cannot describe a year before 0000.
        let (century, in_century) = if year >= 0 {
            (Some(year / 100), Some(year % 100))
        } else {
            (None, None)
        };
        self.year.is_none_or(|given| given == year)
            && self.year_div_100.or(century) == century
            && self.year_mod_100.or(in_century) == in_century
            && self
                .month
                .is_none_or(|given| given == i64::from(date.month()))
            && self.day.is_none_or(|given| given == i64::from(date.day()))
    }

    fn agrees_with_iso_week(&self, date: Date) -> bool {
        let iso = date.iso_week_date();
        let year = i32::from(iso.year());
        let in_century = (year >= 0).then_some(year % 100);
        self.iso_year.is_none_or(|given| given == year)
            && self.iso_year_mod_100.or(in_century) == in_century
            && self
                .iso_week
                .is_none_or(|given| given == i64::from(iso.week()))
            && self.weekday.is_none_or(|given| given == date.weekday())
    }

    fn agrees_with_day_of_year(&self, date: Date) -> bool {
        self.ordinal
            .is_none_or(|given| given == i64::from(date.day_of_year()))
            && self
                .week_from_sun
                .is_none_or(|given| given == weeks_from(date, Weekday::Sunday))
            && self
                .week_from_mon
                .is_none_or(|given| given == weeks_from(date, Weekday::Monday))
    }

    /// The time of day the fields name, and whether it is a leap second.
    fn time(&self) -> ParseResult<(Time, bool)> {
        let half = self.hour_div_12.ok_or(ParseError::NotEnough)?;
        let hour = half * 12 + self.hour_mod_12.ok_or(ParseError::NotEnough)?;
        let minute = self.minute.ok_or(ParseError::NotEnough)?;
        let (second, leap) = match self.second.unwrap_or(0) {
            60 => (59, true),
            second => (second, false),
        };
        // A fraction without the second it belongs to is not a time.
        if self.nanosecond.is_some() && self.second.is_none() {
            return Err(ParseError::NotEnough);
        }
        let narrow = |value: i64| i8::try_from(value).map_err(|_| ParseError::OutOfRange);
        let nanosecond = as_i32(self.nanosecond.unwrap_or(0))?;
        Time::new(narrow(hour)?, narrow(minute)?, narrow(second)?, nanosecond)
            .map(|time| (time, leap))
            .map_err(|_| ParseError::OutOfRange)
    }

    /// The wall-clock time the fields name when read at `offset` seconds
    /// east of UTC. A Unix timestamp fills in whatever the other fields
    /// leave out, and must agree with what they give.
    pub(in crate::nodes::utility) fn date_time(&self, offset: i32) -> ParseResult<Resolved> {
        let (date, time) = (self.date(), self.time());
        if let (Ok(date), Ok((time, leap))) = (date, time) {
            let local = DateTime::from_parts(date, time);
            let seconds = seconds_since_epoch(local) - i64::from(offset);
            let agrees = self
                .timestamp
                .is_none_or(|given| given == seconds || (leap && given == seconds + 1));
            return if agrees {
                Ok(Resolved { local, leap })
            } else {
                Err(ParseError::Impossible)
            };
        }
        let Some(timestamp) = self.timestamp else {
            date?;
            return Err(time.err().unwrap_or(ParseError::NotEnough));
        };
        // Only missing fields are filled in; a wrong one is still an error.
        let failures = [date.err(), time.err()];
        for kind in [ParseError::OutOfRange, ParseError::Impossible] {
            if failures.contains(&Some(kind)) {
                return Err(kind);
            }
        }
        self.date_time_from_timestamp(timestamp, offset)
    }

    fn date_time_from_timestamp(&self, timestamp: i64, offset: i32) -> ParseResult<Resolved> {
        let local = timestamp
            .checked_add(i64::from(offset))
            .and_then(from_epoch_seconds)
            .ok_or(ParseError::OutOfRange)?;
        let mut filled = self.clone();
        let mut local = local;
        if filled.second == Some(60) {
            // The timestamp names the leap second or the second before it.
            match local.second() {
                59 => {}
                0 => {
                    local = from_epoch_seconds(seconds_since_epoch(local) - 1)
                        .ok_or(ParseError::OutOfRange)?
                }
                _ => return Err(ParseError::Impossible),
            }
        } else {
            filled.set_second(i64::from(local.second()))?;
        }
        filled.set_year(i64::from(local.year()))?;
        filled.set_ordinal(i64::from(local.day_of_year()))?;
        filled.set_hour(i64::from(local.hour()))?;
        filled.set_minute(i64::from(local.minute()))?;
        let (time, leap) = filled.time()?;
        Ok(Resolved {
            local: DateTime::from_parts(filled.date()?, time),
            leap,
        })
    }
}

/// The year named by a full year, a century and a two-digit year.
fn resolve_year(
    year: Option<i32>,
    century: Option<i32>,
    in_century: Option<i32>,
) -> ParseResult<Option<i32>> {
    match (year, century, in_century) {
        (year, None, None) => Ok(year),
        (Some(year), century, in_century) => {
            let agrees = year >= 0
                && century.is_none_or(|century| century == year / 100)
                && in_century.is_none_or(|in_century| in_century == year % 100);
            if agrees {
                Ok(Some(year))
            } else {
                Err(ParseError::Impossible)
            }
        }
        (None, Some(century), Some(in_century)) => century
            .checked_mul(100)
            .and_then(|year| year.checked_add(in_century))
            .map(Some)
            .ok_or(ParseError::OutOfRange),
        // Two digits alone: 70 to 99 are 1970 to 1999, 00 to 69 are 2000 to 2069.
        (None, None, Some(in_century)) => {
            Ok(Some(in_century + if in_century < 70 { 2000 } else { 1900 }))
        }
        (None, Some(_), None) => Err(ParseError::NotEnough),
    }
}

fn narrow<T: TryFrom<i64>>(value: Option<i64>) -> ParseResult<T> {
    value
        .and_then(|value| T::try_from(value).ok())
        .ok_or(ParseError::OutOfRange)
}

fn ymd(year: i32, month: Option<i64>, day: Option<i64>) -> ParseResult<Date> {
    let year = i16::try_from(year).map_err(|_| ParseError::OutOfRange)?;
    Date::new(year, narrow(month)?, narrow(day)?).map_err(|_| ParseError::OutOfRange)
}

fn year_day(year: i32, ordinal: Option<i64>) -> ParseResult<Date> {
    let first = ymd(year, Some(1), Some(1))?;
    first
        .with()
        .day_of_year(narrow(ordinal)?)
        .build()
        .map_err(|_| ParseError::OutOfRange)
}

/// The date of `weekday` in week `week`, where week 1 starts on the first
/// `start` weekday of the year.
fn week_date(
    year: i32,
    week: Option<i64>,
    weekday: Option<Weekday>,
    start: Weekday,
) -> ParseResult<Date> {
    let first = ymd(year, Some(1), Some(1))?;
    let (week, weekday) = (
        week.ok_or(ParseError::NotEnough)?,
        weekday.ok_or(ParseError::NotEnough)?,
    );
    let first_week_start = 1 + i64::from(start.since(first.weekday()));
    let ordinal = first_week_start + (week - 1) * 7 + i64::from(weekday.since(start));
    if ordinal <= 0 {
        return Err(ParseError::Impossible);
    }
    year_day(year, Some(ordinal)).map_err(|_| ParseError::Impossible)
}

fn iso_week_date(year: i32, week: Option<i64>, weekday: Option<Weekday>) -> ParseResult<Date> {
    let year = i16::try_from(year).map_err(|_| ParseError::OutOfRange)?;
    let weekday = weekday.ok_or(ParseError::NotEnough)?;
    ISOWeekDate::new(year, narrow(week)?, weekday)
        .map(ISOWeekDate::date)
        .map_err(|_| ParseError::OutOfRange)
}
