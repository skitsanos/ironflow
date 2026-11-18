//! Cron occurrence arithmetic across daylight-saving transitions.
//!
//! Occurrences are enumerated in local wall-clock space and only then resolved
//! to real instants. Iterating in the target zone directly is not equivalent:
//! `cron` drops an occurrence whose local time does not exist, so a daily 02:00
//! job silently does not run on a spring-forward date.
//!
//! Wall-clock times are `jiff::civil::DateTime` and instants are
//! `jiff::Timestamp` or `jiff::ZonedDateTime`, so the two cannot be mixed up.
//! All arithmetic uses `jiff::SignedDuration`, an exact number of seconds:
//! on an instant it is elapsed time, and on a wall-clock time it moves the
//! clock reading by that much with no zone involved. No calendar unit (a
//! "day" that may be 23 or 25 hours) is used anywhere in the scheduler.

use jiff::{SignedDuration, Timestamp, ZonedDateTime, civil};

use super::config::{ScheduleConfig, ScheduleZone};

/// Maximum missed occurrences one tick settles for one schedule, besides the
/// most recent one it decides.
///
/// Bounds the store writes a frequent expression combined with a long
/// catch-up window can create. The remainder is settled on later ticks.
pub const MAX_INSTANTS_PER_TICK: usize = 64;

/// Longest daylight-saving gap this probe will step over. Real gaps are 30 or
/// 60 minutes in every zone tzdata describes; six hours is slack, not a claim
/// about any real zone.
const MAX_GAP_PROBE_MINUTES: i64 = 6 * 60;

/// One occurrence that is due, in both of the forms the scheduler needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueInstant {
    /// Local wall-clock time the expression matched. Identity of the fire.
    pub local: civil::DateTime,
    /// The real instant to fire at, after DST resolution, in the schedule's
    /// zone: `timestamp()` is the instant and `datetime()` the wall-clock
    /// time the zone shows at it.
    pub instant: ZonedDateTime,
    /// Cross-replica claim key derived from `local`.
    pub key: String,
}

/// Resolve a local wall-clock time to the instant it should fire at.
///
/// Returns `None` only if a zone had a gap longer than
/// `MAX_GAP_PROBE_MINUTES`, which no real zone does.
pub fn resolve_local(zone: &ScheduleZone, local: civil::DateTime) -> Option<ZonedDateTime> {
    // An ordinary wall-clock time has one instant. One that repeats when
    // clocks go back has two: fire once, on the earlier — the later one shares
    // a claim key and is refused.
    if let Some(instant) = zone.earliest_instant(local) {
        return Some(instant);
    }
    // Spring forward: this local time never occurs. Fire at the first instant
    // that does, rather than skipping the day.
    let mut probe = local;
    for _ in 0..MAX_GAP_PROBE_MINUTES {
        probe = probe.checked_add(SignedDuration::from_mins(1)).ok()?;
        if let Some(instant) = zone.earliest_instant(probe) {
            return Some(instant);
        }
    }
    None
}

/// Cross-replica identity of one fire.
///
/// Keyed on local wall-clock rather than the UTC instant. On a fall-back date
/// the same local time maps to two distinct UTC instants; a UTC-keyed claim
/// would treat them as different fires and run the schedule twice, which is
/// precisely the duplicate the claim exists to prevent.
///
/// The form is `<zone>@YYYY-MM-DDTHH:MM`. Stored claims and peer replicas
/// compare it byte for byte, so it is written out here rather than left to a
/// library's formatter.
pub fn claim_key(zone: &ScheduleZone, local: civil::DateTime) -> String {
    format!("{}@{}", zone.name(), minute_text(local))
}

/// `YYYY-MM-DDTHH:MM`, the precision a five-field expression can match.
pub(super) fn minute_text(local: civil::DateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}",
        local.year(),
        local.month(),
        local.day(),
        local.hour(),
        local.minute()
    )
}

/// The local instant before which a claim can no longer legitimately fire:
/// `now - grace_seconds`, converted to the schedule's local time. Grace
/// measures real elapsed time, not wall-clock, so the subtraction happens in
/// UTC, before converting to local — the two only agree when the UTC offset
/// is constant across the grace window.
///
/// Shared by `Scheduler::new`, which seeds a schedule's watermark here at
/// startup, and `evaluate`'s claim-error floor below, which stops a
/// permanently failing claim from pinning the watermark past this same
/// horizon.
pub(super) fn grace_floor(now: Timestamp, schedule: &ScheduleConfig) -> civil::DateTime {
    let grace = SignedDuration::from_secs(schedule.grace_seconds_i64());
    // Only a clock within a week of the earliest instant `jiff` represents
    // could overflow; the floor then is that earliest instant.
    let floor = now.checked_sub(grace).unwrap_or(Timestamp::MIN);
    schedule.timezone().local_time(floor)
}

/// The local time to advance a schedule's watermark to for one evaluated
/// window, given the earliest instant inside it that must be retried.
///
/// An instant is retried when its claim errored, so nobody owns it, or when
/// its start was deferred for flow-load capacity, so it is owned but nothing
/// ran. Every other skip (grace, overlap, run capacity) is deliberately
/// burned. The watermark must stop short of a retried instant rather than
/// pass through it. The next tick then re-enumerates from there: instants
/// already settled come back `NotClaimed` and are skipped harmlessly; the
/// retried one gets one attempt per tick until it starts or falls outside
/// grace.
///
/// That cap is floored at `grace_floor`. Grace is only checked *after* a
/// successful claim, so an instant whose claim keeps erroring never reaches
/// that check and never ages out on its own — without the floor it would be
/// retried every tick forever, pinning the watermark while the window in
/// front of it grows without bound. Once a retried instant is older than
/// `grace_floor` it can no longer legitimately fire anyway, so nothing is
/// lost by letting the watermark pass it.
pub(super) fn watermark_target(
    through: civil::DateTime,
    grace_floor: civil::DateTime,
    earliest_retry: Option<civil::DateTime>,
) -> civil::DateTime {
    match earliest_retry {
        Some(errored) => {
            let before = errored
                .checked_sub(SignedDuration::from_secs(1))
                .unwrap_or(civil::DateTime::MIN);
            through.min(before.max(grace_floor))
        }
        None => through,
    }
}

/// Every occurrence in the half-open local window `(after_local,
/// through_local]`, oldest first, resolved to the instant it fires at.
pub(super) fn occurrences(
    schedule: &ScheduleConfig,
    after_local: civil::DateTime,
    through_local: civil::DateTime,
) -> impl Iterator<Item = DueInstant> + '_ {
    let zone = schedule.timezone();
    // The expression is walked in wall-clock space, the space it is written
    // in; each match is resolved to an instant afterwards.
    schedule
        .cron()
        .after(after_local)
        .take_while(move |local| *local <= through_local)
        .filter_map(move |local| {
            let instant = resolve_local(zone, local)?;
            // Keyed on the *resolved* local time, not the matched one: inside a
            // spring-forward gap every matched local (02:00, 02:15, ...) maps to
            // the same real instant (03:00), and they must collapse onto one
            // claim or the gap fires once per gap-interior occurrence instead of
            // once. `DueInstant.local` stays the matched time — `evaluate`'s
            // watermark comparisons need that — only the key changes.
            let key = claim_key(zone, instant.datetime());
            Some(DueInstant {
                local,
                instant,
                key,
            })
        })
}

/// Occurrences due in the half-open local window `(after_local, through_local]`.
///
/// The second return value is `true` when `MAX_INSTANTS_PER_TICK` truncated the
/// result.
pub fn due_instants(
    schedule: &ScheduleConfig,
    after_local: civil::DateTime,
    through_local: civil::DateTime,
) -> (Vec<DueInstant>, bool) {
    if through_local <= after_local {
        return (Vec::new(), false);
    }
    let mut due = Vec::new();
    for instant in occurrences(schedule, after_local, through_local) {
        if due.len() == MAX_INSTANTS_PER_TICK {
            return (due, true);
        }
        due.push(instant);
    }
    (due, false)
}
