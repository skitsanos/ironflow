//! Startup log naming the catch-up window's boundary.
//!
//! `Scheduler::new` seeds each schedule's watermark at `now - grace`, so any
//! occurrence strictly before that point is never enumerated by `evaluate` —
//! not claimed, not logged as `Late`, nothing. This logs that boundary once
//! per schedule at startup so an operator can see where catch-up starts,
//! without asserting that anything before it was actually missed: this
//! process has no way to know whether an earlier one already ran it.

use jiff::{SignedDuration, civil};

use super::config::ScheduleConfig;
use super::timing::minute_text;

/// How far back to search for an occurrence the seeded watermark will skip.
/// Bounds cost, not correctness: a schedule whose last occurrence is older
/// than this is treated as quiet rather than logged about. Thirty-five days
/// of wall-clock time, as an exact length rather than a calendar span.
const LOOKBACK_WINDOW: SignedDuration = SignedDuration::from_hours(35 * 24);

/// Safety cap on how many occurrences one search considers, so a
/// pathologically frequent expression cannot make startup spin. At the
/// five-field dialect's finest grain (one minute), 35 days is at most 50,400
/// occurrences; this leaves headroom without being unbounded.
const MAX_LOOKBACK_INSTANTS: usize = 60_000;

/// The most recent occurrence strictly before `before`, if one falls inside
/// `LOOKBACK_WINDOW`.
///
/// `cron` only iterates forward, so finding the last occurrence before a
/// point means scanning forward from a bounded lower bound and keeping the
/// last hit — there is no cheaper way to ask a `Schedule` this question.
fn last_occurrence_before(
    schedule: &ScheduleConfig,
    before: civil::DateTime,
) -> Option<civil::DateTime> {
    // Walked in wall-clock space, exactly as `due_instants` does.
    let lower_bound = before
        .checked_sub(LOOKBACK_WINDOW)
        .unwrap_or(civil::DateTime::MIN);

    let mut last_missed = None;
    for local in schedule
        .cron()
        .after(lower_bound)
        .take(MAX_LOOKBACK_INSTANTS)
    {
        if local >= before {
            break;
        }
        last_missed = Some(local);
    }
    last_missed
}

/// Log, once, the catch-up window's start and the most recent occurrence
/// before it — or nothing, if none falls inside the lookback window.
pub(super) fn log_unreachable_catchup(
    name: &str,
    schedule: &ScheduleConfig,
    seeded: civil::DateTime,
) {
    if let Some(previous) = last_occurrence_before(schedule, seeded) {
        tracing::info!(
            schedule = %name,
            catch_up_from = %minute_text(seeded),
            previous_occurrence = %minute_text(previous),
            "catch-up window starts here; earlier occurrences are outside it"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(text: &str) -> civil::DateTime {
        text.parse().unwrap()
    }

    fn daily_at_two() -> ScheduleConfig {
        ScheduleConfig::new(
            "f.lua",
            "0 2 * * *",
            Some("UTC"),
            None,
            crate::engine::types::Context::new(),
        )
        .unwrap()
    }

    #[test]
    fn finds_the_most_recent_occurrence_before_the_seed() {
        let schedule = daily_at_two();
        let missed = last_occurrence_before(&schedule, local("2026-07-29 10:00:00"));
        assert_eq!(missed, Some(local("2026-07-29 02:00:00")));
    }

    #[test]
    fn a_seed_before_todays_occurrence_finds_yesterdays() {
        let schedule = daily_at_two();
        let missed = last_occurrence_before(&schedule, local("2026-07-29 01:00:00"));
        assert_eq!(missed, Some(local("2026-07-28 02:00:00")));
    }

    #[test]
    fn a_seed_with_nothing_before_it_yet_finds_nothing() {
        // Nothing has fired before the schedule's very first possible
        // occurrence — the "fresh start on a quiet schedule" case.
        let schedule = ScheduleConfig::new(
            "f.lua",
            "0 2 29 2 *",
            Some("UTC"),
            None,
            crate::engine::types::Context::new(),
        )
        .unwrap();
        let missed = last_occurrence_before(&schedule, local("2026-01-01 00:00:00"));
        assert_eq!(missed, None);
    }

    #[test]
    fn nothing_outside_the_lookback_window_is_found() {
        // A yearly schedule has no occurrence within 35 days of most seeds.
        let schedule = ScheduleConfig::new(
            "f.lua",
            "0 2 1 1 *",
            Some("UTC"),
            None,
            crate::engine::types::Context::new(),
        )
        .unwrap();
        let missed = last_occurrence_before(&schedule, local("2026-07-29 10:00:00"));
        assert_eq!(missed, None);
    }
}
