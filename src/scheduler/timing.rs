//! Cron occurrence arithmetic across daylight-saving transitions.
//!
//! Occurrences are enumerated in local wall-clock space and only then resolved
//! to real instants. Iterating in the target zone directly is not equivalent:
//! `cron` drops an occurrence whose local time does not exist, so a daily 02:00
//! job silently does not run on a spring-forward date.

use chrono::{DateTime, Duration, LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use super::config::ScheduleConfig;

/// Maximum occurrences one tick will consider for one schedule.
///
/// Bounds the work a frequent expression combined with a long catch-up window
/// can create. Truncation is reported rather than silent.
pub const MAX_INSTANTS_PER_TICK: usize = 64;

/// Longest daylight-saving gap this probe will step over. Real gaps are 30 or
/// 60 minutes in every zone tzdata describes; six hours is slack, not a claim
/// about any real zone.
const MAX_GAP_PROBE_MINUTES: i64 = 6 * 60;

/// One occurrence that is due, in both of the forms the scheduler needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueInstant {
    /// Local wall-clock time the expression matched. Identity of the fire.
    pub local: NaiveDateTime,
    /// The real instant to fire at, after DST resolution.
    pub instant: DateTime<Tz>,
    /// Cross-replica claim key derived from `local`.
    pub key: String,
}

/// Resolve a local wall-clock time to the instant it should fire at.
///
/// Returns `None` only if a zone had a gap longer than
/// `MAX_GAP_PROBE_MINUTES`, which no real zone does.
pub fn resolve_local(tz: Tz, local: NaiveDateTime) -> Option<DateTime<Tz>> {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(instant) => Some(instant),
        // Fall back: the wall clock repeats, so this local time has two
        // instants. Fire once, on the earlier — the later one shares a claim
        // key and is refused.
        LocalResult::Ambiguous(earlier, _later) => Some(earlier),
        // Spring forward: this local time never occurs. Fire at the first
        // instant that does, rather than skipping the day.
        LocalResult::None => {
            let mut probe = local;
            for _ in 0..MAX_GAP_PROBE_MINUTES {
                probe += Duration::minutes(1);
                match tz.from_local_datetime(&probe) {
                    LocalResult::Single(instant) => return Some(instant),
                    LocalResult::Ambiguous(earlier, _) => return Some(earlier),
                    LocalResult::None => {}
                }
            }
            None
        }
    }
}

/// Cross-replica identity of one fire.
///
/// Keyed on local wall-clock rather than the UTC instant. On a fall-back date
/// the same local time maps to two distinct UTC instants; a UTC-keyed claim
/// would treat them as different fires and run the schedule twice, which is
/// precisely the duplicate the claim exists to prevent.
pub fn claim_key(tz: Tz, local: NaiveDateTime) -> String {
    format!("{}@{}", tz.name(), local.format("%Y-%m-%dT%H:%M"))
}

/// Occurrences due in the half-open local window `(after_local, through_local]`.
///
/// The second return value is `true` when `MAX_INSTANTS_PER_TICK` truncated the
/// result.
pub fn due_instants(
    schedule: &ScheduleConfig,
    after_local: NaiveDateTime,
    through_local: NaiveDateTime,
) -> (Vec<DueInstant>, bool) {
    if through_local <= after_local {
        return (Vec::new(), false);
    }

    let tz = schedule.timezone();
    // Feed naive local times to `cron` as if they were UTC. The iterator then
    // walks wall-clock occurrences, which is the space the expression is
    // written in.
    let cursor = Utc.from_utc_datetime(&after_local);

    let mut due = Vec::new();
    let mut truncated = false;
    for candidate in schedule.cron().after(&cursor) {
        let local = candidate.naive_utc();
        if local > through_local {
            break;
        }
        if due.len() == MAX_INSTANTS_PER_TICK {
            truncated = true;
            break;
        }
        if let Some(instant) = resolve_local(tz, local) {
            due.push(DueInstant {
                local,
                instant,
                key: claim_key(tz, local),
            });
        }
    }

    (due, truncated)
}
