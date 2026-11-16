//! The due instants of one evaluation, split by the catch-up rule (IF-217).
//!
//! When more than one instant of a schedule is due at once, the schedule has
//! missed some: the process was down, or an earlier tick could not decide.
//! Starting them back to back was never a backlog. The first run made every
//! later instant overlap it, so which instant ran was an accident of order
//! and the rest were lost. The rule is therefore explicit: the most recent
//! due instant is decided as usual, and every earlier one is superseded,
//! recorded as settled and never started.

use chrono::NaiveDateTime;

use super::config::ScheduleConfig;
use super::timing::{self, DueInstant, MAX_INSTANTS_PER_TICK};

/// Most occurrences one evaluation enumerates. The window is at most the
/// grace window plus a tick, which at one-minute granularity and the largest
/// accepted grace is 10,081 occurrences, so this bound is slack that only
/// keeps a defect elsewhere from turning into an unbounded loop.
const MAX_WINDOW_OCCURRENCES: usize = 16_384;

pub(super) struct DueWindow {
    /// The most recent due occurrence. One instant, except inside a
    /// daylight-saving gap, where several matched local times resolve to the
    /// same instant and share its claim key.
    pub current: Vec<DueInstant>,
    /// Earlier occurrences that `current` supersedes, oldest first, at most
    /// `MAX_INSTANTS_PER_TICK` of them.
    pub superseded: Vec<DueInstant>,
    /// Local time of the oldest superseded occurrence that did not fit in
    /// this tick. The watermark must stop before it.
    pub resume_at: Option<NaiveDateTime>,
    /// Local time the enumeration reached: `through_local`, unless
    /// `MAX_WINDOW_OCCURRENCES` cut it short.
    pub through: NaiveDateTime,
}

/// Split the occurrences due in `(after_local, through_local]`.
pub(super) fn due_window(
    schedule: &ScheduleConfig,
    after_local: NaiveDateTime,
    through_local: NaiveDateTime,
) -> DueWindow {
    let mut window = DueWindow {
        current: Vec::new(),
        superseded: Vec::new(),
        resume_at: None,
        through: through_local,
    };
    let occurrences = timing::occurrences(schedule, after_local, through_local);
    for (seen, instant) in occurrences.enumerate() {
        if seen == MAX_WINDOW_OCCURRENCES {
            window.through = window
                .current
                .last()
                .map_or(through_local, |last| last.local);
            break;
        }
        let same_occurrence = window
            .current
            .first()
            .is_none_or(|current| current.key == instant.key);
        if !same_occurrence {
            // A more recent occurrence: what was current is now superseded.
            for earlier in window.current.drain(..) {
                if window.superseded.len() < MAX_INSTANTS_PER_TICK {
                    window.superseded.push(earlier);
                } else {
                    window.resume_at.get_or_insert(earlier.local);
                }
            }
        }
        window.current.push(instant);
    }
    window
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    fn schedule(cron: &str, zone: &str) -> ScheduleConfig {
        let ctx = crate::engine::types::Context::new();
        ScheduleConfig::new("f.lua", cron, Some(zone), None, ctx).unwrap()
    }

    fn keys(instants: &[DueInstant]) -> Vec<&str> {
        instants
            .iter()
            .map(|instant| instant.key.as_str())
            .collect()
    }

    #[test]
    fn one_due_instant_supersedes_nothing() {
        let window = due_window(
            &schedule("* * * * *", "UTC"),
            local("2026-05-01 03:59:30"),
            local("2026-05-01 04:00:10"),
        );
        assert_eq!(keys(&window.current), ["UTC@2026-05-01T04:00"]);
        assert!(window.superseded.is_empty());
        assert_eq!(window.resume_at, None);
        assert_eq!(window.through, local("2026-05-01 04:00:10"));
    }

    #[test]
    fn nothing_due_is_an_empty_window() {
        let window = due_window(
            &schedule("0 2 * * *", "UTC"),
            local("2026-05-01 03:00:00"),
            local("2026-05-01 04:00:00"),
        );
        assert!(window.current.is_empty() && window.superseded.is_empty());
        let backwards = due_window(
            &schedule("* * * * *", "UTC"),
            local("2026-05-01 04:00:00"),
            local("2026-05-01 03:00:00"),
        );
        assert!(backwards.current.is_empty() && backwards.superseded.is_empty());
    }

    #[test]
    fn the_most_recent_instant_is_current_and_the_rest_are_superseded() {
        let window = due_window(
            &schedule("*/15 * * * *", "UTC"),
            local("2026-05-01 03:00:00"),
            local("2026-05-01 04:00:10"),
        );
        assert_eq!(keys(&window.current), ["UTC@2026-05-01T04:00"]);
        assert_eq!(
            keys(&window.superseded),
            [
                "UTC@2026-05-01T03:15",
                "UTC@2026-05-01T03:30",
                "UTC@2026-05-01T03:45",
            ]
        );
        assert_eq!(window.resume_at, None);
    }

    #[test]
    fn a_long_window_keeps_the_latest_and_resumes_after_the_oldest_batch() {
        // 03:01 to 06:00 is 180 occurrences: 179 superseded, 64 this tick.
        let window = due_window(
            &schedule("* * * * *", "UTC"),
            local("2026-05-01 03:00:10"),
            local("2026-05-01 06:00:10"),
        );
        assert_eq!(keys(&window.current), ["UTC@2026-05-01T06:00"]);
        assert_eq!(window.superseded.len(), MAX_INSTANTS_PER_TICK);
        assert_eq!(window.superseded[0].key, "UTC@2026-05-01T03:01");
        assert_eq!(window.superseded[63].key, "UTC@2026-05-01T04:04");
        assert_eq!(window.resume_at, Some(local("2026-05-01 04:05:00")));
    }

    #[test]
    fn matched_times_inside_a_spring_forward_gap_stay_one_occurrence() {
        // 02:00 to 02:45 do not exist in Berlin on 2026-03-29; with 03:00 they
        // all resolve to 03:00 and share its key, so none supersedes another.
        let window = due_window(
            &schedule("*/15 1-3 * * *", "Europe/Berlin"),
            local("2026-03-29 01:20:00"),
            local("2026-03-29 03:05:00"),
        );
        assert_eq!(window.current.len(), 5, "{:?}", keys(&window.current));
        assert!(
            window
                .current
                .iter()
                .all(|instant| instant.key == "Europe/Berlin@2026-03-29T03:00")
        );
        assert_eq!(
            keys(&window.superseded),
            [
                "Europe/Berlin@2026-03-29T01:30",
                "Europe/Berlin@2026-03-29T01:45",
            ]
        );
    }
}
