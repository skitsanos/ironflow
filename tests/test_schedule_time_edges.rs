//! Edges of the scheduler's time arithmetic that a change of time library
//! could move (IF-231): sub-second values, the exact ends of the half-open
//! window `(after, through]`, grace exactly at its limit, and the first and
//! last years the cron evaluator enumerates (1970 and 2100).
//!
//! Every expectation here was recorded from the `chrono` implementation
//! before the scheduler's own types moved to `jiff`, and none was changed by
//! the move: only the three helpers that build a time were.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ironflow::engine::types::Context;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::timing::{claim_key, due_instants, resolve_local};
use ironflow::scheduler::{Outcome, ScheduleExecutor, ScheduleRun, Scheduler};
use ironflow::storage::null_store::NullStateStore;
use jiff::{SignedDuration, Timestamp, civil};

type Local = civil::DateTime;
type Instant = Timestamp;

/// A wall-clock time such as `2026-05-01 02:00:00` or `2026-05-01 02:00:00.5`.
fn local(text: &str) -> Local {
    text.parse().unwrap()
}

/// The UTC instant written as a wall-clock time.
fn at(text: &str) -> Instant {
    jiff::tz::Offset::UTC.to_timestamp(local(text)).unwrap()
}

/// The smallest step either library represents.
fn one_nanosecond_before(time: Local) -> Local {
    time - SignedDuration::from_nanos(1)
}

fn schedule(cron: &str, zone: &str, grace_seconds: Option<u64>) -> ScheduleConfig {
    ScheduleConfig::new("f.lua", cron, Some(zone), grace_seconds, Context::new()).unwrap()
}

fn keys(schedule: &ScheduleConfig, after: Local, through: Local) -> Vec<String> {
    let (due, truncated) = due_instants(schedule, after, through);
    assert!(!truncated);
    due.into_iter().map(|instant| instant.key).collect()
}

#[derive(Default)]
struct CountingExecutor {
    runs: AtomicUsize,
}

#[async_trait::async_trait]
impl ScheduleExecutor for CountingExecutor {
    async fn active_run(&self, _schedule_name: &str) -> Option<String> {
        None
    }

    fn has_capacity(&self) -> bool {
        true
    }

    async fn run(
        &self,
        _schedule_name: &str,
        _instant_key: &str,
        _schedule: &ScheduleConfig,
    ) -> Result<ScheduleRun, String> {
        let index = self.runs.fetch_add(1, Ordering::SeqCst);
        Ok(ScheduleRun::Started {
            run_id: format!("run-{index}"),
        })
    }
}

fn scheduler(schedule: ScheduleConfig, started: Instant) -> Scheduler {
    Scheduler::new(
        HashMap::from([("job".to_string(), schedule)]),
        Arc::new(NullStateStore::new()),
        Arc::new(CountingExecutor::default()),
        started,
    )
}

/// `(key, outcome)` of every decision of one evaluation.
async fn decided(scheduler: &mut Scheduler, now: Instant) -> Vec<(String, Outcome)> {
    scheduler
        .evaluate(now)
        .await
        .into_iter()
        .map(|decision| (decision.key, decision.outcome))
        .collect()
}

fn fired(key: &str, run: usize) -> (String, Outcome) {
    let run_id = format!("run-{run}");
    (key.to_string(), Outcome::Fired { run_id })
}

#[test]
fn the_window_excludes_its_start_and_includes_its_end_to_the_nanosecond() {
    let hourly = schedule("0 * * * *", "UTC", None);
    let four = local("2026-05-01 04:00:00");
    let five = local("2026-05-01 05:00:00");

    // `after` is exclusive: an occurrence exactly on it was already decided.
    assert_eq!(keys(&hourly, four, five), ["UTC@2026-05-01T05:00"]);
    // One nanosecond earlier and it is inside again.
    assert_eq!(
        keys(&hourly, one_nanosecond_before(four), five),
        ["UTC@2026-05-01T04:00", "UTC@2026-05-01T05:00"]
    );
    // `through` is inclusive, and one nanosecond short of it is not due yet.
    assert_eq!(
        keys(&hourly, four, one_nanosecond_before(five)),
        Vec::<String>::new()
    );
    // A window of no width, or a sub-second one between occurrences, is empty.
    assert_eq!(keys(&hourly, five, five), Vec::<String>::new());
    assert_eq!(
        keys(
            &hourly,
            local("2026-05-01 04:00:00.25"),
            local("2026-05-01 04:00:00.75")
        ),
        Vec::<String>::new()
    );
}

#[test]
fn sub_second_window_ends_keep_their_fraction_in_a_zone_with_an_offset() {
    // 02:00 in Berlin on 2026-05-01 is 00:00 UTC. The fraction must survive
    // the conversion to wall-clock time: rounding either end to a whole
    // second would move an occurrence across the boundary.
    let nightly = schedule("0 2 * * *", "Europe/Berlin", None);
    let (due, _) = due_instants(
        &nightly,
        local("2026-05-01 01:59:59.999999999"),
        local("2026-05-01 02:00:00"),
    );
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].local, local("2026-05-01 02:00:00"));
    assert_eq!(due[0].key, "Europe/Berlin@2026-05-01T02:00");
}

#[test]
fn the_evaluator_starts_at_1970_and_stops_after_2100() {
    let yearly = schedule("0 0 1 1 *", "UTC", None);
    // Nothing is enumerated before 1970, however early the window starts.
    assert_eq!(
        keys(
            &yearly,
            local("1968-06-01 00:00:00"),
            local("1971-06-01 00:00:00")
        ),
        ["UTC@1970-01-01T00:00", "UTC@1971-01-01T00:00"]
    );
    assert_eq!(
        keys(
            &yearly,
            local("1900-01-01 00:00:00"),
            local("1969-12-31 23:59:59")
        ),
        Vec::<String>::new()
    );
    // 2100 is the last year `cron` 0.17 enumerates; a window that runs past
    // it simply ends.
    assert_eq!(
        keys(
            &yearly,
            local("2098-06-01 00:00:00"),
            local("2105-06-01 00:00:00")
        ),
        ["UTC@2099-01-01T00:00", "UTC@2100-01-01T00:00"]
    );
    assert_eq!(
        keys(
            &yearly,
            local("2101-01-01 00:00:00"),
            local("2200-01-01 00:00:00")
        ),
        Vec::<String>::new()
    );

    let last_minute = schedule("59 23 31 12 *", "UTC", None);
    assert_eq!(
        keys(
            &last_minute,
            local("2100-12-31 23:00:00"),
            local("2101-01-01 01:00:00")
        ),
        ["UTC@2100-12-31T23:59"]
    );
    let first_minute = schedule("* * * * *", "UTC", None);
    assert_eq!(
        keys(
            &first_minute,
            local("1969-12-31 23:58:30"),
            local("1970-01-01 00:01:30")
        ),
        ["UTC@1970-01-01T00:00", "UTC@1970-01-01T00:01"]
    );
}

#[test]
fn the_year_limits_hold_in_a_zone_whose_local_year_differs_from_utc() {
    // Local 2100-12-31 23:59 in Auckland is 10:59 UTC the same day, and local
    // 1970-01-01 00:00 in New York is 05:00 UTC: both are ordinary instants.
    let auckland = schedule("59 23 31 12 *", "Pacific/Auckland", None);
    let (due, _) = due_instants(
        &auckland,
        local("2100-12-31 00:00:00"),
        local("2101-01-01 12:00:00"),
    );
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].key, "Pacific/Auckland@2100-12-31T23:59");

    let new_york = schedule("0 0 1 1 *", "America/New_York", None);
    let (due, _) = due_instants(
        &new_york,
        local("1969-12-01 00:00:00"),
        local("1970-06-01 00:00:00"),
    );
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].key, "America/New_York@1970-01-01T00:00");
}

#[tokio::test]
async fn lateness_is_whole_elapsed_seconds_so_grace_holds_through_its_last_second() {
    // Grace is 300 seconds and the instant is 02:00:00. Lateness drops the
    // fraction, so 300.999 seconds late is still 300 and still fires.
    let nightly = || schedule("0 2 * * *", "UTC", None);
    let started = at("2026-05-01 01:59:00");

    let mut exact = scheduler(nightly(), started);
    assert_eq!(
        decided(&mut exact, at("2026-05-01 02:05:00")).await,
        [fired("UTC@2026-05-01T02:00", 0)]
    );

    let mut last_fraction = scheduler(nightly(), started);
    assert_eq!(
        decided(&mut last_fraction, at("2026-05-01 02:05:00.999999999")).await,
        [fired("UTC@2026-05-01T02:00", 0)]
    );

    for late in ["2026-05-01 02:05:01", "2026-05-01 02:05:01.999999999"] {
        let mut past = scheduler(nightly(), started);
        assert_eq!(
            decided(&mut past, at(late)).await,
            [(
                "UTC@2026-05-01T02:00".to_string(),
                Outcome::Late {
                    lateness_seconds: 301
                }
            )],
            "{late}"
        );
    }
}

#[tokio::test]
async fn an_evaluation_a_fraction_before_the_instant_decides_nothing() {
    let mut scheduler = scheduler(
        schedule("0 2 * * *", "Europe/Berlin", None),
        at("2026-04-30 23:59:00"),
    );
    // 02:00 Berlin is 00:00 UTC.
    assert_eq!(
        decided(&mut scheduler, at("2026-04-30 23:59:59.999999999")).await,
        []
    );
    assert_eq!(
        decided(&mut scheduler, at("2026-05-01 00:00:00")).await,
        [fired("Europe/Berlin@2026-05-01T02:00", 0)]
    );
    // Zero elapsed time since the last evaluation decides nothing again.
    assert_eq!(decided(&mut scheduler, at("2026-05-01 00:00:00")).await, []);
}

#[tokio::test]
async fn the_startup_watermark_is_now_minus_grace_to_the_fraction() {
    // Grace is 60 seconds. The watermark is exclusive, so whether the
    // occurrence exactly one grace back is caught up depends on the fraction.
    let minutely = || schedule("* * * * *", "UTC", Some(60));

    // Started on the minute: the watermark is 02:00:00 and excludes 02:00.
    let mut on_the_minute = scheduler(minutely(), at("2026-05-01 02:01:00"));
    assert_eq!(
        decided(&mut on_the_minute, at("2026-05-01 02:01:00")).await,
        [fired("UTC@2026-05-01T02:01", 0)]
    );

    // Started half a second later: the watermark is 02:00:00.5.
    let mut after_the_minute = scheduler(minutely(), at("2026-05-01 02:01:00.5"));
    assert_eq!(
        decided(&mut after_the_minute, at("2026-05-01 02:01:00.5")).await,
        [fired("UTC@2026-05-01T02:01", 0)]
    );

    // Started a millisecond earlier: the watermark is 01:59:59.999, so 02:00
    // is inside the window and the more recent 02:01 supersedes it.
    let mut before_the_minute = scheduler(minutely(), at("2026-05-01 02:00:59.999"));
    assert_eq!(
        decided(&mut before_the_minute, at("2026-05-01 02:01:00")).await,
        [
            (
                "UTC@2026-05-01T02:00".to_string(),
                Outcome::Superseded {
                    by: "UTC@2026-05-01T02:01".to_string()
                }
            ),
            fired("UTC@2026-05-01T02:01", 0),
        ]
    );
}

#[test]
fn a_wall_clock_time_resolves_up_to_the_last_instant_jiff_represents() {
    // Not recorded from `chrono`, whose instants ran on past the year 9999:
    // this is the one place the move to `jiff` changed a result (IF-231).
    // `jiff::Timestamp` ends 26 hours before the end of 9999 and starts 26
    // hours after the start of -9999, so that every instant has a wall-clock
    // time under every offset. No schedule reaches either end, because the
    // cron evaluator covers 1970 to 2100.
    let utc = schedule("0 0 * * *", "UTC", None);
    let zone = utc.timezone();

    let last = resolve_local(zone, local("9999-12-30 22:00:00")).unwrap();
    assert_eq!(last.timestamp().as_second(), Timestamp::MAX.as_second());
    assert_eq!(claim_key(zone, last.datetime()), "UTC@9999-12-30T22:00");
    assert_eq!(resolve_local(zone, local("9999-12-30 22:00:01")), None);
    assert_eq!(resolve_local(zone, local("9999-12-31 23:59:59")), None);

    let first = resolve_local(zone, local("-009999-01-02 01:59:59")).unwrap();
    assert_eq!(first.timestamp(), Timestamp::MIN);

    // In a zone ahead of UTC the last wall-clock time is later by its offset.
    let auckland = schedule("0 0 * * *", "Pacific/Auckland", None);
    let zone = auckland.timezone();
    assert!(resolve_local(zone, local("9999-12-31 11:00:00")).is_some());
    assert_eq!(resolve_local(zone, local("9999-12-31 11:00:01")), None);
}
