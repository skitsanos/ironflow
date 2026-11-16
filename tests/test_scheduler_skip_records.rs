//! IF-217 (g): an instant the scheduler skipped is consumed durably.
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

mod json {
    use super::*;

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
