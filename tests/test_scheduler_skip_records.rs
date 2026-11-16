//! IF-217 (c) and (g): catch-up starts the most recent missed instant only,
//! and an instant the scheduler skipped is consumed durably.
//!
//! A skip creates no run, so the claim alone used to be its only trace, and a
//! lost claim without a run record reads as an orphan that IF-078 recovers.
//! A replica that restarted inside the grace window therefore started the
//! instant its peer had deliberately skipped. Every case here shares one real
//! store between two schedulers and counts the starts each one asks for.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, TimeZone as _, Utc};
use ironflow::engine::types::Context;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::{Decision, Outcome, ScheduleExecutor, ScheduleRun, Scheduler};
use ironflow::storage::StateStore;

#[cfg(feature = "redis")]
#[path = "support/redis.rs"]
mod redis_support;

#[cfg(feature = "postgres")]
#[path = "support/postgres.rs"]
mod postgres_support;

const NAME: &str = "report";

/// What one replica's executor answers. It never touches a store: the
/// scheduler's own claim and occurrence records are what is under test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Behaviour {
    Start,
    Overlap,
    NoCapacityAtProbe,
    NoCapacityAtStart,
    FailToLoad,
    Defer,
}

struct Replica {
    behaviour: Behaviour,
    starts: AtomicUsize,
}

impl Replica {
    fn new(behaviour: Behaviour) -> Arc<Self> {
        Arc::new(Self {
            behaviour,
            starts: AtomicUsize::new(0),
        })
    }

    fn starts(&self) -> usize {
        self.starts.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl ScheduleExecutor for Replica {
    async fn active_run(&self, _schedule_name: &str) -> Option<String> {
        (self.behaviour == Behaviour::Overlap).then(|| "schedule-earlier-run".to_string())
    }

    fn has_capacity(&self) -> bool {
        self.behaviour != Behaviour::NoCapacityAtProbe
    }

    async fn run(
        &self,
        _schedule_name: &str,
        instant_key: &str,
        _schedule: &ScheduleConfig,
    ) -> Result<ScheduleRun, String> {
        match self.behaviour {
            Behaviour::FailToLoad => Err("failed to load flow: syntax error".to_string()),
            Behaviour::NoCapacityAtStart => Ok(ScheduleRun::AtCapacity),
            Behaviour::Defer => Ok(ScheduleRun::Deferred {
                reason: "load limit full".to_string(),
            }),
            _ => {
                self.starts.fetch_add(1, Ordering::SeqCst);
                Ok(ScheduleRun::Started {
                    run_id: format!("run-{instant_key}"),
                })
            }
        }
    }
}

fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 1, hour, minute, second)
        .unwrap()
}

/// A replica's scheduler, started at `started` and so catching up over the
/// grace window before it, like a restarted process.
fn scheduler(
    cron: &str,
    grace_seconds: u64,
    store: &Arc<dyn StateStore>,
    replica: &Arc<Replica>,
    started: DateTime<Utc>,
) -> Scheduler {
    let schedule = ScheduleConfig::new(
        "report.lua",
        cron,
        Some("UTC"),
        Some(grace_seconds),
        Context::new(),
    )
    .unwrap();
    Scheduler::new(
        HashMap::from([(NAME.to_string(), schedule)]),
        store.clone(),
        replica.clone(),
        started,
    )
}

fn outcomes(decisions: &[Decision]) -> Vec<(&str, &Outcome)> {
    decisions
        .iter()
        .map(|decision| (decision.key.as_str(), &decision.outcome))
        .collect()
}

const HOURLY: &str = "0 * * * *";
const TWO_O_CLOCK: &str = "UTC@2026-05-01T02:00";

/// One replica skips the 02:00 instant for `skip`; a second replica that
/// starts ten minutes later, inside the hour of grace and with nothing in its
/// way, must not start it.
async fn a_skipped_instant_is_not_started_by_a_restarted_replica(
    store: Arc<dyn StateStore>,
    skip: Behaviour,
) {
    let first = Replica::new(skip);
    let decisions = scheduler(HOURLY, 3_000, &store, &first, at(2, 0, 5))
        .evaluate(at(2, 0, 5))
        .await;
    assert_eq!(decisions.len(), 1, "{decisions:?}");
    assert_eq!(decisions[0].key, TWO_O_CLOCK);
    let expected = match skip {
        Behaviour::Overlap => matches!(decisions[0].outcome, Outcome::Overlapped { .. }),
        Behaviour::NoCapacityAtProbe | Behaviour::NoCapacityAtStart => {
            decisions[0].outcome == Outcome::AtCapacity
        }
        Behaviour::FailToLoad => matches!(decisions[0].outcome, Outcome::Failed { .. }),
        other => panic!("{other:?} is not a skip"),
    };
    assert!(expected, "{skip:?} produced {decisions:?}");
    assert_eq!(first.starts(), 0);

    let restarted = Replica::new(Behaviour::Start);
    let decisions = scheduler(HOURLY, 3_000, &store, &restarted, at(2, 10, 0))
        .evaluate(at(2, 10, 0))
        .await;

    assert_eq!(
        outcomes(&decisions),
        [(TWO_O_CLOCK, &Outcome::NotClaimed)],
        "the instant skipped for {skip:?} must stay consumed"
    );
    assert_eq!(restarted.starts(), 0, "nothing may start after {skip:?}");
}

async fn every_skip_is_durable(store: impl Fn() -> Arc<dyn StateStore>) {
    for skip in [
        Behaviour::Overlap,
        Behaviour::NoCapacityAtProbe,
        Behaviour::NoCapacityAtStart,
        Behaviour::FailToLoad,
    ] {
        a_skipped_instant_is_not_started_by_a_restarted_replica(store(), skip).await;
    }
}

/// The control: a deferral is a delay. Nothing was consumed, so the instant
/// is still started, by the same replica or by one that restarts.
async fn a_deferred_instant_is_still_started(store: Arc<dyn StateStore>) {
    let first = Replica::new(Behaviour::Defer);
    let decisions = scheduler(HOURLY, 3_000, &store, &first, at(2, 0, 5))
        .evaluate(at(2, 0, 5))
        .await;
    assert!(
        matches!(decisions[0].outcome, Outcome::Deferred { .. }),
        "{decisions:?}"
    );

    let restarted = Replica::new(Behaviour::Start);
    let decisions = scheduler(HOURLY, 3_000, &store, &restarted, at(2, 10, 0))
        .evaluate(at(2, 10, 0))
        .await;

    assert!(
        matches!(decisions[0].outcome, Outcome::Fired { .. }),
        "{decisions:?}"
    );
    assert_eq!(restarted.starts(), 1);
}

// -- IF-217 (c): catch-up runs the most recent missed instant only ----------

const EVERY_MINUTE: &str = "* * * * *";

fn minute_key(hour: u32, minute: u32) -> String {
    format!("UTC@2026-05-01T{hour:02}:{minute:02}")
}

/// Evaluate `scheduler` at `now` until a tick records no further superseded
/// instant, and return every decision in order. One tick records a bounded
/// number of them, so a long catch-up takes several.
async fn catch_up(scheduler: &mut Scheduler, now: DateTime<Utc>) -> Vec<Decision> {
    let mut all = Vec::new();
    for _tick in 0..32 {
        let decisions = scheduler.evaluate(now).await;
        let superseded = decisions
            .iter()
            .filter(|decision| matches!(decision.outcome, Outcome::Superseded { .. }))
            .count();
        all.extend(decisions);
        if superseded == 0 {
            return all;
        }
    }
    panic!("catch-up did not finish: {} decisions", all.len());
}

/// A schedule that missed `missed` every-minute instants starts one run, for
/// the most recent, and settles every other one durably. A second replica
/// that starts inside the grace window starts none of them.
async fn catch_up_starts_only_the_latest_instant(store: Arc<dyn StateStore>, missed: u32) {
    // Grace reaches back exactly `missed` instants from 04:00:10.
    let grace = u64::from(missed) * 60;
    let now = at(4, 0, 10);
    let latest = minute_key(4, 0);

    let first = Replica::new(Behaviour::Start);
    let decisions = catch_up(
        &mut scheduler(EVERY_MINUTE, grace, &store, &first, now),
        now,
    )
    .await;

    assert_eq!(first.starts(), 1, "exactly one run starts at catch-up");
    let fired = decisions
        .iter()
        .filter(|decision| matches!(decision.outcome, Outcome::Fired { .. }))
        .map(|decision| decision.key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        fired,
        [latest.as_str()],
        "the run is for the latest instant"
    );
    let superseded = decisions
        .iter()
        .filter(|decision| decision.outcome == Outcome::Superseded { by: latest.clone() })
        .map(|decision| decision.key.clone())
        .collect::<Vec<_>>();
    let expected = (1..missed)
        .map(|back| {
            let instant = at(4, 0, 0) - chrono::Duration::minutes(i64::from(missed - back));
            format!("UTC@{}", instant.format("%Y-%m-%dT%H:%M"))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        superseded, expected,
        "every other instant is superseded once"
    );
    for key in expected.iter().chain([&latest]) {
        assert!(
            store.schedule_run_recorded(NAME, key).await.unwrap(),
            "{key} must have a durable occurrence record"
        );
    }

    // A second replica starts thirty seconds later, inside the grace window.
    let later = at(4, 0, 40);
    let second = Replica::new(Behaviour::Start);
    let decisions = catch_up(
        &mut scheduler(EVERY_MINUTE, grace, &store, &second, later),
        later,
    )
    .await;

    assert_eq!(second.starts(), 0, "the second replica starts nothing");
    let not_settled = decisions
        .iter()
        .filter(|decision| decision.outcome != Outcome::NotClaimed)
        .collect::<Vec<_>>();
    assert!(not_settled.is_empty(), "{not_settled:?}");
}

/// The control for the rule itself: without a missed instant nothing is
/// superseded, and the next minute is an ordinary start.
async fn a_single_due_instant_is_not_superseded(store: Arc<dyn StateStore>) {
    let replica = Replica::new(Behaviour::Start);
    let mut scheduler = scheduler(EVERY_MINUTE, 60, &store, &replica, at(4, 0, 10));

    let first = scheduler.evaluate(at(4, 0, 10)).await;
    let second = scheduler.evaluate(at(4, 0, 40)).await;
    let third = scheduler.evaluate(at(4, 1, 10)).await;

    assert!(
        matches!(outcomes(&first)[..], [(key, Outcome::Fired { .. })] if key == minute_key(4, 0)),
        "{first:?}"
    );
    assert!(second.is_empty(), "{second:?}");
    assert!(
        matches!(outcomes(&third)[..], [(key, Outcome::Fired { .. })] if key == minute_key(4, 1)),
        "{third:?}"
    );
    assert_eq!(replica.starts(), 2);
}

mod json {
    use super::*;

    #[tokio::test]
    async fn catch_up_starts_only_the_latest_instant() {
        let dir = tempfile::tempdir().unwrap();
        super::catch_up_starts_only_the_latest_instant(store(&dir, "few"), 30).await;
        super::catch_up_starts_only_the_latest_instant(store(&dir, "many"), 150).await;
        super::a_single_due_instant_is_not_superseded(store(&dir, "one")).await;
    }

    fn store(dir: &tempfile::TempDir, name: &str) -> Arc<dyn StateStore> {
        Arc::new(ironflow::storage::json_store::JsonStateStore::new(
            dir.path().join(name),
        ))
    }

    #[tokio::test]
    async fn every_skip_is_durable() {
        let dir = tempfile::tempdir().unwrap();
        let next = AtomicUsize::new(0);
        super::every_skip_is_durable(|| {
            store(&dir, &next.fetch_add(1, Ordering::SeqCst).to_string())
        })
        .await;
    }

    #[tokio::test]
    async fn a_deferred_instant_is_still_started() {
        let dir = tempfile::tempdir().unwrap();
        super::a_deferred_instant_is_still_started(store(&dir, "deferred")).await;
    }
}

mod sqlite {
    use super::*;

    async fn store(dir: &tempfile::TempDir, name: &str) -> Arc<dyn StateStore> {
        let url = format!("sqlite://{}/{name}.sqlite?mode=rwc", dir.path().display());
        Arc::new(
            ironflow::storage::sql_store::SqlStateStore::new(&url)
                .await
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn catch_up_starts_only_the_latest_instant() {
        let dir = tempfile::tempdir().unwrap();
        super::catch_up_starts_only_the_latest_instant(store(&dir, "few").await, 30).await;
        super::catch_up_starts_only_the_latest_instant(store(&dir, "many").await, 150).await;
        super::a_single_due_instant_is_not_superseded(store(&dir, "one").await).await;
    }

    #[tokio::test]
    async fn every_skip_is_durable() {
        let dir = tempfile::tempdir().unwrap();
        for skip in [
            Behaviour::Overlap,
            Behaviour::NoCapacityAtProbe,
            Behaviour::NoCapacityAtStart,
            Behaviour::FailToLoad,
        ] {
            let store = store(&dir, &format!("{skip:?}")).await;
            a_skipped_instant_is_not_started_by_a_restarted_replica(store, skip).await;
        }
    }

    #[tokio::test]
    async fn a_deferred_instant_is_still_started() {
        let dir = tempfile::tempdir().unwrap();
        super::a_deferred_instant_is_still_started(store(&dir, "deferred").await).await;
    }
}

#[cfg(feature = "postgres")]
mod postgres {
    use super::postgres_support::PostgresStateTest;
    use super::*;

    /// Run one case against an isolated PostgreSQL table prefix.
    pub(super) async fn run<F, Fut>(label: &str, case: F)
    where
        F: FnOnce(Arc<dyn StateStore>) -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let Some(fixture) = PostgresStateTest::from_env(label) else {
            return;
        };
        let store: Arc<dyn StateStore> = Arc::new(fixture.state_store().await.unwrap());
        let result = tokio::spawn(case(store)).await;
        fixture.cleanup().await.unwrap();
        result.unwrap();
    }

    #[tokio::test]
    async fn catch_up_starts_only_the_latest_instant() {
        run("pg_catchup_few", |store| {
            super::catch_up_starts_only_the_latest_instant(store, 30)
        })
        .await;
        run("pg_catchup_many", |store| {
            super::catch_up_starts_only_the_latest_instant(store, 150)
        })
        .await;
    }

    #[tokio::test]
    async fn every_skip_is_durable() {
        for (label, skip) in [
            ("pg_skip_overlap", Behaviour::Overlap),
            ("pg_skip_probe", Behaviour::NoCapacityAtProbe),
            ("pg_skip_start", Behaviour::NoCapacityAtStart),
            ("pg_skip_load", Behaviour::FailToLoad),
        ] {
            run(label, move |store| {
                a_skipped_instant_is_not_started_by_a_restarted_replica(store, skip)
            })
            .await;
        }
    }

    #[tokio::test]
    async fn a_deferred_instant_is_still_started() {
        run(
            "pg_skip_deferred",
            super::a_deferred_instant_is_still_started,
        )
        .await;
    }
}

#[cfg(feature = "redis")]
mod redis {
    use super::redis_support::RedisTest;
    use super::*;

    /// Run one case against an isolated Redis key prefix.
    pub(super) async fn run<F, Fut>(label: &str, case: F)
    where
        F: FnOnce(Arc<dyn StateStore>) -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let Some(fixture) = RedisTest::connect(label).await else {
            return;
        };
        let store: Arc<dyn StateStore> = Arc::new(fixture.state_store(None).await);
        let result = tokio::spawn(case(store)).await;
        fixture.cleanup().await;
        result.unwrap();
    }

    #[tokio::test]
    async fn catch_up_starts_only_the_latest_instant() {
        run("catchup_few", |store| {
            super::catch_up_starts_only_the_latest_instant(store, 30)
        })
        .await;
        run("catchup_many", |store| {
            super::catch_up_starts_only_the_latest_instant(store, 150)
        })
        .await;
    }

    #[tokio::test]
    async fn every_skip_is_durable() {
        for (label, skip) in [
            ("skip_overlap", Behaviour::Overlap),
            ("skip_probe", Behaviour::NoCapacityAtProbe),
            ("skip_start", Behaviour::NoCapacityAtStart),
            ("skip_load", Behaviour::FailToLoad),
        ] {
            run(label, move |store| {
                a_skipped_instant_is_not_started_by_a_restarted_replica(store, skip)
            })
            .await;
        }
    }

    #[tokio::test]
    async fn a_deferred_instant_is_still_started() {
        run("skip_deferred", super::a_deferred_instant_is_still_started).await;
    }
}
