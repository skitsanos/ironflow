//! The boundary with the `cron` crate, and the only scheduler module that
//! uses `chrono` (IF-231).
//!
//! `cron` evaluates an expression over `chrono` date-times. The scheduler asks
//! it one question, "which wall-clock time after this one matches next?", and
//! hands it wall-clock times labelled as UTC. `cron` therefore walks the space
//! the expression is written in and never applies a zone rule: resolving a
//! wall-clock time to an instant is `ScheduleZone`'s job, not this crate's.
//!
//! Both conversions are exact. A `jiff` civil date-time (years -9999 to 9999,
//! nanosecond precision, no leap second) always fits `chrono`'s wider range,
//! and `cron` only returns whole seconds in the years 1970 to 2100.

use std::str::FromStr;

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use jiff::civil;

/// One parsed expression in the six-field form `cron` reads.
#[derive(Clone, Debug)]
pub(super) struct Expression(cron::Schedule);

impl Expression {
    /// Parse `seconds minute hour day month weekday`. `None` when `cron`
    /// rejects the expression.
    pub(super) fn parse(six_fields: &str) -> Option<Self> {
        cron::Schedule::from_str(six_fields).ok().map(Self)
    }

    /// The first matching wall-clock time strictly after `after`, or `None`
    /// once the expression has no later match (`cron` stops after 2100).
    pub(super) fn next_after(&self, after: civil::DateTime) -> Option<civil::DateTime> {
        let cursor = naive_as_utc(after)?;
        self.0.after(&cursor).next().and_then(wall_clock)
    }
}

/// The wall-clock time `local`, labelled as UTC for `cron`.
fn naive_as_utc(local: civil::DateTime) -> Option<DateTime<Utc>> {
    let field = |value: i8| u32::try_from(value).ok();
    let date = NaiveDate::from_ymd_opt(
        i32::from(local.year()),
        field(local.month())?,
        field(local.day())?,
    )?;
    let time = date.and_hms_nano_opt(
        field(local.hour())?,
        field(local.minute())?,
        field(local.second())?,
        u32::try_from(local.subsec_nanosecond()).ok()?,
    )?;
    Some(time.and_utc())
}

/// The wall-clock time `cron` returned under its UTC label.
fn wall_clock(candidate: DateTime<Utc>) -> Option<civil::DateTime> {
    let naive = candidate.naive_utc();
    let field = |value: u32| i8::try_from(value).ok();
    civil::DateTime::new(
        i16::try_from(naive.year()).ok()?,
        field(naive.month())?,
        field(naive.day())?,
        field(naive.hour())?,
        field(naive.minute())?,
        field(naive.second())?,
        i32::try_from(naive.nanosecond()).ok()?,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_minute() -> Expression {
        Expression::parse("0 * * * * *").unwrap()
    }

    #[test]
    fn conversion_is_exact_at_both_ends_of_the_civil_range() {
        for local in [
            civil::DateTime::MIN,
            civil::DateTime::MAX,
            civil::datetime(1970, 1, 1, 0, 0, 0, 0),
            civil::datetime(2024, 2, 29, 23, 59, 59, 999_999_999),
        ] {
            let labelled = naive_as_utc(local).unwrap();
            assert_eq!(wall_clock(labelled), Some(local), "{local}");
        }
    }

    #[test]
    fn a_time_outside_the_civil_range_or_on_a_leap_second_has_no_wall_clock() {
        let beyond = NaiveDate::from_ymd_opt(10_000, 1, 1).unwrap();
        assert_eq!(
            wall_clock(beyond.and_hms_opt(0, 0, 0).unwrap().and_utc()),
            None
        );
        // `chrono` writes a leap second as nanosecond 1_000_000_000 and up.
        let leap = NaiveDate::from_ymd_opt(2016, 12, 31).unwrap();
        let leap = leap.and_hms_nano_opt(23, 59, 59, 1_000_000_000).unwrap();
        assert_eq!(wall_clock(leap.and_utc()), None);
    }

    #[test]
    fn the_next_match_is_strictly_after_the_cursor_to_the_nanosecond() {
        let minutely = every_minute();
        let four = civil::datetime(2026, 5, 1, 4, 0, 0, 0);
        let just_before = civil::datetime(2026, 5, 1, 3, 59, 59, 999_999_999);
        let just_after = civil::datetime(2026, 5, 1, 4, 0, 0, 1);
        let next = civil::datetime(2026, 5, 1, 4, 1, 0, 0);
        assert_eq!(minutely.next_after(just_before), Some(four));
        assert_eq!(minutely.next_after(four), Some(next));
        assert_eq!(minutely.next_after(just_after), Some(next));
    }

    #[test]
    fn matches_end_with_the_last_year_cron_enumerates() {
        let minutely = every_minute();
        let last = civil::datetime(2100, 12, 31, 23, 59, 0, 0);
        assert_eq!(
            minutely.next_after(civil::datetime(2100, 12, 31, 23, 58, 30, 0)),
            Some(last)
        );
        assert_eq!(minutely.next_after(last), None);
        assert_eq!(minutely.next_after(civil::DateTime::MAX), None);
        // Before the first year the next match is the first one of 1970.
        assert_eq!(
            minutely.next_after(civil::datetime(1900, 1, 1, 0, 0, 0, 0)),
            Some(civil::datetime(1970, 1, 1, 0, 0, 0, 0))
        );
    }

    #[test]
    fn an_expression_cron_rejects_does_not_parse() {
        assert!(Expression::parse("0 61 * * * *").is_none());
    }
}
