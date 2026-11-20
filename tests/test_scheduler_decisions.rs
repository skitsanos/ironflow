use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ironflow::engine::types::Context;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::{Outcome, ScheduleExecutor, ScheduleRun, Scheduler};
use ironflow::storage::StateStore;
use ironflow::storage::null_store::NullStateStore;
use jiff::{SignedDuration, Timestamp};

#[path = "scheduler/fall_back.rs"]
mod fall_back;

#[derive(Default)]
struct StubExecutor {
    runs: AtomicUsize,
    active: std::sync::Mutex<Option<String>>,
    at_capacity: AtomicBool,
    fail: AtomicBool,
    existing: AtomicBool,
    defer: AtomicBool,
}

#[async_trait::async_trait]
impl ScheduleExecutor for StubExecutor {
    async fn active_run(&self, _schedule_name: &str) -> Option<String> {
        self.active.lock().unwrap().clone()
    }

    fn has_capacity(&self) -> bool {
        !self.at_capacity.load(Ordering::SeqCst)
    }

    async fn run(
        &self,
        _schedule_name: &str,
        _instant_key: &str,
        _schedule: &ScheduleConfig,
    ) -> Result<ScheduleRun, String> {
        if self.fail.load(Ordering::SeqCst) {
            return Err("flow blew up".to_string());
        }
        if self.defer.load(Ordering::SeqCst) {
            return Ok(ScheduleRun::Deferred {
                reason: "load limit full".to_string(),
            });
        }
        let index = self.runs.fetch_add(1, Ordering::SeqCst);
        if self.existing.load(Ordering::SeqCst) {
            return Ok(ScheduleRun::Existing {
                run_id: "run-owned-by-peer".to_string(),
            });
        }
        Ok(ScheduleRun::Started {
            run_id: format!("run-{index}"),
        })
    }
}

fn utc(y: i16, m: i8, d: i8, h: i8, min: i8) -> Timestamp {
    utc_at(y, m, d, h, min, 0)
}

fn utc_at(year: i16, month: i8, day: i8, hour: i8, minute: i8, second: i8) -> Timestamp {
    let wall_clock = jiff::civil::datetime(year, month, day, hour, minute, second, 0);
    jiff::tz::Offset::UTC.to_timestamp(wall_clock).unwrap()
}

fn daily_at_two() -> HashMap<String, ScheduleConfig> {
    HashMap::from([(
        "nightly".to_string(),
        ScheduleConfig::new(
            "nightly.lua",
            "0 2 * * *",
            Some("UTC"),
            None,
            Context::new(),
        )
        .unwrap(),
    )])
}

fn scheduler(
    store: Arc<dyn StateStore>,
    executor: Arc<StubExecutor>,
    start: Timestamp,
) -> Scheduler {
    Scheduler::new(daily_at_two(), store, executor, start)
}

#[tokio::test]
async fn an_instant_inside_the_window_fires() {
    let executor = Arc::new(StubExecutor::default());
    let mut scheduler = scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 1, 1, 59),
    );

    let decisions = scheduler.evaluate(utc(2026, 5, 1, 2, 0)).await;

    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].schedule, "nightly");
    assert_eq!(decisions[0].key, "UTC@2026-05-01T02:00");
    assert!(matches!(&decisions[0].outcome, Outcome::Fired { run_id } if run_id == "run-0"));
    assert_eq!(executor.runs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn nothing_fires_between_instants() {
    let executor = Arc::new(StubExecutor::default());
    // Constructed at 02:30, well clear of the 02:00 instant: the watermark
    // seeds at 02:25, so catch-up cannot reach back to it. This isolates
    // "nothing is due between instants" from the catch-up behaviour that
    // `a_process_down_briefly_across_an_instant_fires_on_restart` covers.
    let mut scheduler = scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 1, 2, 30),
    );

    assert!(scheduler.evaluate(utc(2026, 5, 1, 2, 35)).await.is_empty());
    assert!(scheduler.evaluate(utc(2026, 5, 1, 3, 0)).await.is_empty());
    assert_eq!(executor.runs.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_process_down_briefly_across_an_instant_fires_on_restart() {
    // A restart seeds the watermark at `now - grace`, so an instant missed
    // during a short outage is still inside the window and runs.
    let executor = Arc::new(StubExecutor::default());
    let mut scheduler = scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        // Restarting at 02:03, three minutes after the 02:00 instant, with the
        // 300-second default grace.
        utc(2026, 5, 1, 2, 3),
    );

    let decisions = scheduler.evaluate(utc(2026, 5, 1, 2, 3)).await;

    assert_eq!(decisions.len(), 1);
    assert!(
        matches!(decisions[0].outcome, Outcome::Fired { .. }),
        "a brief outage should not lose the instant: {decisions:?}"
    );
}

#[tokio::test]
async fn an_instant_fires_only_once_across_consecutive_ticks() {
    let executor = Arc::new(StubExecutor::default());
    let mut scheduler = scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 1, 1, 59),
    );

    scheduler.evaluate(utc(2026, 5, 1, 2, 0)).await;
    scheduler.evaluate(utc(2026, 5, 1, 2, 1)).await;
    scheduler.evaluate(utc(2026, 5, 1, 2, 2)).await;

    assert_eq!(executor.runs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn losing_the_claim_converges_on_the_peer_run() {
    // A real store with the instant already claimed, rather than a stub: this
    // exercises the actual claim path a peer replica would have taken.
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ironflow::storage::json_store::JsonStateStore::new(
        dir.path(),
    ));
    store
        .claim_schedule("nightly", "UTC@2026-05-01T02:00", 604_800)
        .await
        .unwrap();

    let executor = Arc::new(StubExecutor::default());
    executor.existing.store(true, Ordering::SeqCst);
    let mut scheduler = scheduler(
        store.clone() as Arc<dyn StateStore>,
        executor.clone(),
        utc(2026, 5, 1, 1, 59),
    );

    let decisions = scheduler.evaluate(utc(2026, 5, 1, 2, 0)).await;

    assert_eq!(decisions.len(), 1);
    assert!(matches!(decisions[0].outcome, Outcome::NotClaimed));
    assert_eq!(executor.runs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_instant_outside_the_grace_window_is_skipped_and_names_its_lateness() {
    let executor = Arc::new(StubExecutor::default());
    // Default grace is 300s. Start the scheduler well before the instant, then
    // evaluate long after it.
    let mut scheduler = scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 1, 1, 59),
    );

    let decisions = scheduler.evaluate(utc(2026, 5, 1, 4, 0)).await;

    assert_eq!(decisions.len(), 1);
    match &decisions[0].outcome {
        Outcome::Late { lateness_seconds } => assert_eq!(*lateness_seconds, 7_200),
        other => panic!("expected Late, got {other:?}"),
    }
    assert_eq!(executor.runs.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_instant_just_inside_the_grace_window_still_fires() {
    let executor = Arc::new(StubExecutor::default());
    let mut scheduler = scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 1, 1, 59),
    );

    // 299 seconds late, one second inside the 300-second default.
    let decisions = scheduler
        .evaluate(utc(2026, 5, 1, 2, 4) + SignedDuration::from_secs(59))
        .await;

    assert!(matches!(decisions[0].outcome, Outcome::Fired { .. }));
    assert_eq!(executor.runs.load(Ordering::SeqCst), 1);

    // Exactly at the boundary: `>` means equal lateness is still inside the
    // window, so this must fire too.
    // (`scheduler` the local binding above now shadows the `scheduler` helper
    // fn, so the helper is reached via its module path here.)
    let executor = Arc::new(StubExecutor::default());
    let mut scheduler = self::scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 1, 1, 59),
    );
    let decisions = scheduler.evaluate(utc(2026, 5, 1, 2, 5)).await;
    assert!(
        matches!(decisions[0].outcome, Outcome::Fired { .. }),
        "lateness of exactly grace_seconds should fire, got {:?}",
        decisions[0].outcome
    );
    assert_eq!(executor.runs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_still_running_previous_run_causes_a_skip() {
    let executor = Arc::new(StubExecutor::default());
    *executor.active.lock().unwrap() = Some("run-earlier".to_string());
    let mut scheduler = scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 1, 1, 59),
    );

    let decisions = scheduler.evaluate(utc(2026, 5, 1, 2, 0)).await;

    match &decisions[0].outcome {
        Outcome::Overlapped { active_run } => assert_eq!(active_run, "run-earlier"),
        other => panic!("expected Overlapped, got {other:?}"),
    }
    assert_eq!(executor.runs.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_saturated_server_skips_rather_than_queuing() {
    let executor = Arc::new(StubExecutor::default());
    executor.at_capacity.store(true, Ordering::SeqCst);
    let mut scheduler = scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 1, 1, 59),
    );

    let decisions = scheduler.evaluate(utc(2026, 5, 1, 2, 0)).await;

    assert!(matches!(decisions[0].outcome, Outcome::AtCapacity));
    assert_eq!(executor.runs.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_failing_flow_fails_that_run_and_not_the_scheduler() {
    let executor = Arc::new(StubExecutor::default());
    executor.fail.store(true, Ordering::SeqCst);
    let mut scheduler = scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 1, 1, 59),
    );

    let first = scheduler.evaluate(utc(2026, 5, 1, 2, 0)).await;
    assert!(matches!(first[0].outcome, Outcome::Failed { .. }));

    // The next day's instant must still be evaluated.
    executor.fail.store(false, Ordering::SeqCst);
    let second = scheduler.evaluate(utc(2026, 5, 2, 2, 0)).await;
    assert!(matches!(second[0].outcome, Outcome::Fired { .. }));
}

#[tokio::test]
async fn every_configured_schedule_is_evaluated_in_one_tick() {
    // Neither schedule is made to fail here; that guarantee is covered by
    // `a_failing_flow_fails_that_run_and_not_the_scheduler`. This only checks
    // that a single tick evaluates every configured schedule, not just the
    // first.
    let mut schedules = daily_at_two();
    schedules.insert(
        "hourly".to_string(),
        ScheduleConfig::new("hourly.lua", "0 * * * *", Some("UTC"), None, Context::new()).unwrap(),
    );
    let executor = Arc::new(StubExecutor::default());
    let mut scheduler = Scheduler::new(
        schedules,
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 1, 1, 59),
    );

    let decisions = scheduler.evaluate(utc(2026, 5, 1, 2, 0)).await;

    assert_eq!(decisions.len(), 2, "both schedules must be evaluated");
    assert!(
        decisions
            .iter()
            .all(|d| matches!(d.outcome, Outcome::Fired { .. }))
    );
}

#[tokio::test]
async fn startup_catch_up_is_bounded_by_the_grace_window() {
    // A scheduler constructed long after an instant must not replay history:
    // seeding at `now - grace` means only instants that could still fire are
    // considered.
    let executor = Arc::new(StubExecutor::default());
    let mut scheduler = scheduler(
        Arc::new(NullStateStore::new()),
        executor.clone(),
        utc(2026, 5, 10, 12, 0),
    );

    let decisions = scheduler.evaluate(utc(2026, 5, 10, 12, 0)).await;

    assert!(decisions.is_empty(), "replayed history: {decisions:?}");
    assert_eq!(executor.runs.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn catch_up_seeding_measures_real_elapsed_time_across_a_dst_gap() {
    // Berlin springs forward at 02:00 local (01:00 UTC) on 2026-03-29, so at
    // 01:30 UTC the local clock already reads 03:30 CEST.
    //
    // Grace is a span of real elapsed time, so the watermark must be seeded by
    // subtracting it from the UTC instant and only then converting to local.
    // Subtracting an hour from the local wall clock instead yields 02:30 — a
    // local time that does not exist that day — and silently drops this 01:45
    // instant, which is only 45 minutes late against a 60-minute grace.
    let executor = Arc::new(StubExecutor::default());
    let schedule = ScheduleConfig::new(
        "nightly.lua",
        "45 1 * * *",
        Some("Europe/Berlin"),
        Some(3600),
        Context::new(),
    )
    .unwrap();
    let now = utc_at(2026, 3, 29, 1, 30, 0);

    let mut scheduler = Scheduler::new(
        HashMap::from([("nightly".to_string(), schedule)]),
        Arc::new(NullStateStore::new()),
        executor.clone(),
        now,
    );

    let decisions = scheduler.evaluate(now).await;

    assert_eq!(
        decisions.len(),
        1,
        "the 01:45 instant is within grace and should be caught up: {decisions:?}"
    );
    assert!(
        matches!(decisions[0].outcome, Outcome::Fired { .. }),
        "expected Fired, got {:?}",
        decisions[0].outcome
    );
}

/// Wraps a real store and fails the *first* `claim_schedule` call, then
/// delegates every call after that — simulating a transient failure (a
/// Postgres failover, a Redis reconnect) straddling one instant.
struct FlakyClaimStore {
    inner: Arc<ironflow::storage::json_store::JsonStateStore>,
    claim_calls: AtomicUsize,
}

#[async_trait::async_trait]
impl StateStore for FlakyClaimStore {
    async fn init_run(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.init_run(run_id, flow_name, ctx).await
    }
    async fn set_run_status(
        &self,
        run_id: &str,
        status: ironflow::engine::types::RunStatus,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.set_run_status(run_id, status).await
    }
    async fn upsert_task(
        &self,
        run_id: &str,
        task: &ironflow::engine::types::TaskState,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.upsert_task(run_id, task).await
    }
    async fn get_ctx(&self, run_id: &str) -> ironflow::storage::StorageResult<Context> {
        self.inner.get_ctx(run_id).await
    }
    async fn update_ctx(
        &self,
        run_id: &str,
        ctx: &Context,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.update_ctx(run_id, ctx).await
    }
    async fn get_run_info(
        &self,
        run_id: &str,
    ) -> ironflow::storage::StorageResult<ironflow::engine::types::RunInfo> {
        self.inner.get_run_info(run_id).await
    }
    async fn list_runs(
        &self,
        status: Option<ironflow::engine::types::RunStatus>,
    ) -> ironflow::storage::StorageResult<Vec<ironflow::engine::types::RunInfo>> {
        self.inner.list_runs(status).await
    }
    async fn list_run_summaries_page(
        &self,
        query: &ironflow::storage::RunListQuery,
    ) -> ironflow::storage::StorageResult<ironflow::storage::RunSummaryPage> {
        self.inner.list_run_summaries_page(query).await
    }
    async fn delete_run(&self, run_id: &str) -> ironflow::storage::StorageResult<()> {
        self.inner.delete_run(run_id).await
    }
    async fn claim_schedule(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> ironflow::storage::StorageResult<bool> {
        if self.claim_calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(ironflow::storage::StorageError::backend(
                "Claim scheduled instant",
                "simulated transient store failure",
            ));
        }
        self.inner.claim_schedule(name, key, ttl_seconds).await
    }
}

#[tokio::test]
async fn a_claim_error_is_retried_on_the_next_evaluate_instead_of_burned() {
    let dir = tempfile::tempdir().unwrap();
    let store: Arc<dyn StateStore> = Arc::new(FlakyClaimStore {
        inner: Arc::new(ironflow::storage::json_store::JsonStateStore::new(
            dir.path(),
        )),
        claim_calls: AtomicUsize::new(0),
    });

    let executor = Arc::new(StubExecutor::default());
    let mut scheduler = scheduler(store, executor.clone(), utc(2026, 5, 1, 1, 59));

    // First tick: the store errors while claiming, so nobody owns the
    // instant. It must not be burned the way a claimed-then-skipped instant
    // is.
    let first = scheduler.evaluate(utc(2026, 5, 1, 2, 0)).await;
    assert_eq!(first.len(), 1);
    assert!(
        matches!(&first[0].outcome, Outcome::ClaimFailed { .. }),
        "expected ClaimFailed, got {:?}",
        first[0].outcome
    );
    assert_eq!(executor.runs.load(Ordering::SeqCst), 0);

    // Second tick, well inside grace: the watermark was capped below the
    // errored instant, so it is re-enumerated — and this time the claim
    // (now hitting the real store for the first time) succeeds and it fires.
    let second = scheduler.evaluate(utc(2026, 5, 1, 2, 1)).await;
    assert_eq!(second.len(), 1);
    assert!(
        matches!(&second[0].outcome, Outcome::Fired { .. }),
        "expected Fired, got {:?}",
        second[0].outcome
    );
    assert_eq!(executor.runs.load(Ordering::SeqCst), 1);
}

/// Wraps a real store and fails every `claim_schedule` call, forever — a
/// store that never recovers, unlike `FlakyClaimStore`'s one-shot failure.
struct AlwaysErroringClaimStore {
    inner: Arc<ironflow::storage::json_store::JsonStateStore>,
}

#[async_trait::async_trait]
impl StateStore for AlwaysErroringClaimStore {
    async fn init_run(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.init_run(run_id, flow_name, ctx).await
    }
    async fn set_run_status(
        &self,
        run_id: &str,
        status: ironflow::engine::types::RunStatus,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.set_run_status(run_id, status).await
    }
    async fn upsert_task(
        &self,
        run_id: &str,
        task: &ironflow::engine::types::TaskState,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.upsert_task(run_id, task).await
    }
    async fn get_ctx(&self, run_id: &str) -> ironflow::storage::StorageResult<Context> {
        self.inner.get_ctx(run_id).await
    }
    async fn update_ctx(
        &self,
        run_id: &str,
        ctx: &Context,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.update_ctx(run_id, ctx).await
    }
    async fn get_run_info(
        &self,
        run_id: &str,
    ) -> ironflow::storage::StorageResult<ironflow::engine::types::RunInfo> {
        self.inner.get_run_info(run_id).await
    }
    async fn list_runs(
        &self,
        status: Option<ironflow::engine::types::RunStatus>,
    ) -> ironflow::storage::StorageResult<Vec<ironflow::engine::types::RunInfo>> {
        self.inner.list_runs(status).await
    }
    async fn list_run_summaries_page(
        &self,
        query: &ironflow::storage::RunListQuery,
    ) -> ironflow::storage::StorageResult<ironflow::storage::RunSummaryPage> {
        self.inner.list_run_summaries_page(query).await
    }
    async fn delete_run(&self, run_id: &str) -> ironflow::storage::StorageResult<()> {
        self.inner.delete_run(run_id).await
    }
    async fn claim_schedule(
        &self,
        _name: &str,
        _key: &str,
        _ttl_seconds: u64,
    ) -> ironflow::storage::StorageResult<bool> {
        Err(ironflow::storage::StorageError::backend(
            "Claim scheduled instant",
            "simulated permanent store outage",
        ))
    }
}

#[tokio::test]
async fn an_instant_aged_out_of_grace_stops_pinning_the_watermark() {
    // A store that never recovers: every claim errors, forever. Grace is
    // only checked *after* a successful claim (see `decide`), so nothing
    // stops a permanently failing claim from being retried on every tick
    // except the floor this test targets: `timing::watermark_target` capping
    // the watermark no lower than `now - grace`, so an instant that can no
    // longer legitimately fire eventually stops pinning it.
    let dir = tempfile::tempdir().unwrap();
    let store: Arc<dyn StateStore> = Arc::new(AlwaysErroringClaimStore {
        inner: Arc::new(ironflow::storage::json_store::JsonStateStore::new(
            dir.path(),
        )),
    });

    let schedules = HashMap::from([(
        "frequent".to_string(),
        ScheduleConfig::new("f.lua", "* * * * *", Some("UTC"), None, Context::new()).unwrap(),
    )]);

    let executor = Arc::new(StubExecutor::default());
    // Seeded so the watermark sits one second before the 02:00:00 instant —
    // the state a prior tick would have left behind right after that
    // instant's claim first errored (mirrors the pinned watermark in
    // `a_claim_error_is_retried_on_the_next_evaluate_instead_of_burned`).
    let mut scheduler = Scheduler::new(
        schedules,
        store,
        executor.clone(),
        utc(2026, 5, 1, 2, 4) + SignedDuration::from_secs(59),
    );

    let stale_key = "UTC@2026-05-01T02:00";

    // Tick near the instant: it is the only thing due, and its claim fails.
    let first = scheduler
        .evaluate(utc(2026, 5, 1, 2, 0) + SignedDuration::from_secs(15))
        .await;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].key, stale_key);
    assert!(matches!(first[0].outcome, Outcome::ClaimFailed { .. }));

    // Tick well past grace: the store is still down, so this necessarily
    // re-processes the whole backlog accumulated since the pinned watermark,
    // stale instant included — that growth is the bug this finding
    // describes, and no single tick can avoid it once a backlog exists. What
    // the fix changes is whether the *next* tick keeps doing this.
    let second = scheduler
        .evaluate(utc(2026, 5, 1, 2, 40) + SignedDuration::from_secs(15))
        .await;
    assert!(
        second.iter().any(|d| d.key == stale_key),
        "sanity check: the backlog tick must still see the stale instant"
    );

    // The next regular tick, 30 seconds later: on unfloored code the
    // watermark is still pinned at 01:59:59 (nothing ever ages it out), so
    // this re-enumerates the same ~41-instant backlog, stale key included,
    // and the window only grows further from here on every subsequent tick.
    // Floored, the watermark advanced past grace during the previous tick,
    // so this tick's window is small and the stale instant is gone for good.
    let third = scheduler
        .evaluate(utc(2026, 5, 1, 2, 40) + SignedDuration::from_secs(45))
        .await;
    assert!(
        !third.iter().any(|d| d.key == stale_key),
        "the long-past instant was re-enumerated instead of aging out: {third:?}"
    );
    assert!(
        third.len() < second.len(),
        "the window kept growing instead of catching up: tick2 had {} decisions, tick3 had {}",
        second.len(),
        third.len()
    );
}

#[tokio::test]
async fn a_deferred_start_is_retried_and_only_the_latest_missed_instant_fires() {
    let dir = tempfile::tempdir().unwrap();
    let store: Arc<dyn StateStore> = Arc::new(ironflow::storage::json_store::JsonStateStore::new(
        dir.path(),
    ));
    let schedules = HashMap::from([(
        "frequent".to_string(),
        ScheduleConfig::new("f.lua", "* * * * *", Some("UTC"), None, Context::new()).unwrap(),
    )]);
    let executor = Arc::new(StubExecutor::default());
    let start = utc(2026, 5, 1, 2, 0) + SignedDuration::from_secs(30);
    // The default 300-second grace seeds a five-instant catch-up window.
    let mut scheduler = Scheduler::new(schedules, store, executor.clone(), start);
    let outcomes = |decisions: &[ironflow::scheduler::Decision]| {
        decisions
            .iter()
            .map(|d| (d.key.clone(), d.outcome.clone()))
            .collect::<Vec<_>>()
    };
    let superseded = |key: &str, by: &str| {
        let by = format!("UTC@2026-05-01T{by}");
        (format!("UTC@2026-05-01T{key}"), Outcome::Superseded { by })
    };

    // IF-217 (c): at catch-up only the most recent instant is decided. IF-182:
    // its start is deferred for flow-load capacity, so it is claimed but
    // never ran and must not be burned.
    executor.defer.store(true, Ordering::SeqCst);
    let first = scheduler.evaluate(start).await;
    assert_eq!(
        outcomes(&first)[..4],
        [
            superseded("01:56", "02:00"),
            superseded("01:57", "02:00"),
            superseded("01:58", "02:00"),
            superseded("01:59", "02:00"),
        ]
    );
    assert_eq!(first.len(), 5, "{first:?}");
    assert_eq!(first[4].key, "UTC@2026-05-01T02:00");
    assert!(matches!(first[4].outcome, Outcome::Deferred { .. }));
    assert_eq!(executor.runs.load(Ordering::SeqCst), 0);

    // The deferred instant is retried on the next tick and fires.
    executor.defer.store(false, Ordering::SeqCst);
    let second = scheduler
        .evaluate(start + SignedDuration::from_secs(15))
        .await;
    assert_eq!(second.len(), 1, "{second:?}");
    assert_eq!(second[0].key, "UTC@2026-05-01T02:00");
    assert!(matches!(second[0].outcome, Outcome::Fired { .. }));
    assert_eq!(executor.runs.load(Ordering::SeqCst), 1);

    let third = scheduler
        .evaluate(start + SignedDuration::from_secs(30))
        .await;
    assert_eq!(third.len(), 1, "{third:?}");
    assert_eq!(third[0].key, "UTC@2026-05-01T02:01");
    assert!(matches!(third[0].outcome, Outcome::Fired { .. }));
    assert_eq!(executor.runs.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn an_instant_still_deferred_when_the_next_is_due_is_superseded_by_it() {
    let dir = tempfile::tempdir().unwrap();
    let store: Arc<dyn StateStore> = Arc::new(ironflow::storage::json_store::JsonStateStore::new(
        dir.path(),
    ));
    let schedules = HashMap::from([(
        "frequent".to_string(),
        ScheduleConfig::new("f.lua", "* * * * *", Some("UTC"), None, Context::new()).unwrap(),
    )]);
    let executor = Arc::new(StubExecutor::default());
    let start = utc(2026, 5, 1, 2, 0) + SignedDuration::from_secs(5);
    let mut scheduler = Scheduler::new(schedules, store, executor.clone(), start);
    // Settle the catch-up window first, so only 02:00 is due below.
    scheduler
        .evaluate(start - SignedDuration::from_secs(10))
        .await;
    let before = executor.runs.load(Ordering::SeqCst);

    executor.defer.store(true, Ordering::SeqCst);
    let first = scheduler.evaluate(start).await;
    assert!(
        matches!(first.last().unwrap().outcome, Outcome::Deferred { .. }),
        "{first:?}"
    );

    // A minute later the limit is free again and 02:01 is due as well: the
    // deferred 02:00 is a missed instant now, and 02:01 is the one that runs.
    executor.defer.store(false, Ordering::SeqCst);
    let second = scheduler
        .evaluate(start + SignedDuration::from_secs(60))
        .await;
    let outcomes = second
        .iter()
        .map(|d| (d.key.as_str(), &d.outcome))
        .collect::<Vec<_>>();
    assert!(
        matches!(
            outcomes[..],
            [
                ("UTC@2026-05-01T02:00", Outcome::Superseded { by }),
                ("UTC@2026-05-01T02:01", Outcome::Fired { .. }),
            ] if by == "UTC@2026-05-01T02:01"
        ),
        "{second:?}"
    );
    assert_eq!(executor.runs.load(Ordering::SeqCst), before + 1);
}

/// Wraps a real store and injects failures into the occurrence record
/// (IF-201): a number of failing `schedule_run_recorded` reads, and failing
/// `record_schedule_run` writes.
struct FaultyRecordStore {
    inner: Arc<ironflow::storage::json_store::JsonStateStore>,
    failing_reads: AtomicUsize,
    fail_writes: AtomicBool,
}

impl FaultyRecordStore {
    fn new(dir: &std::path::Path) -> Self {
        Self {
            inner: Arc::new(ironflow::storage::json_store::JsonStateStore::new(dir)),
            failing_reads: AtomicUsize::new(0),
            fail_writes: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl StateStore for FaultyRecordStore {
    async fn init_run(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.init_run(run_id, flow_name, ctx).await
    }
    async fn set_run_status(
        &self,
        run_id: &str,
        status: ironflow::engine::types::RunStatus,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.set_run_status(run_id, status).await
    }
    async fn upsert_task(
        &self,
        run_id: &str,
        task: &ironflow::engine::types::TaskState,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.upsert_task(run_id, task).await
    }
    async fn get_ctx(&self, run_id: &str) -> ironflow::storage::StorageResult<Context> {
        self.inner.get_ctx(run_id).await
    }
    async fn update_ctx(
        &self,
        run_id: &str,
        ctx: &Context,
    ) -> ironflow::storage::StorageResult<()> {
        self.inner.update_ctx(run_id, ctx).await
    }
    async fn get_run_info(
        &self,
        run_id: &str,
    ) -> ironflow::storage::StorageResult<ironflow::engine::types::RunInfo> {
        self.inner.get_run_info(run_id).await
    }
    async fn list_runs(
        &self,
        status: Option<ironflow::engine::types::RunStatus>,
    ) -> ironflow::storage::StorageResult<Vec<ironflow::engine::types::RunInfo>> {
        self.inner.list_runs(status).await
    }
    async fn list_run_summaries_page(
        &self,
        query: &ironflow::storage::RunListQuery,
    ) -> ironflow::storage::StorageResult<ironflow::storage::RunSummaryPage> {
        self.inner.list_run_summaries_page(query).await
    }
    async fn delete_run(&self, run_id: &str) -> ironflow::storage::StorageResult<()> {
        self.inner.delete_run(run_id).await
    }
    async fn claim_schedule(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> ironflow::storage::StorageResult<bool> {
        self.inner.claim_schedule(name, key, ttl_seconds).await
    }
    async fn record_schedule_run(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> ironflow::storage::StorageResult<()> {
        if self.fail_writes.load(Ordering::SeqCst) {
            return Err(ironflow::storage::StorageError::backend(
                "Record scheduled run",
                "simulated store failure",
            ));
        }
        self.inner.record_schedule_run(name, key, ttl_seconds).await
    }
    async fn schedule_run_recorded(
        &self,
        name: &str,
        key: &str,
    ) -> ironflow::storage::StorageResult<bool> {
        let failing = self
            .failing_reads
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok();
        if failing {
            return Err(ironflow::storage::StorageError::backend(
                "Read scheduled run record",
                "simulated store failure",
            ));
        }
        self.inner.schedule_run_recorded(name, key).await
    }
}

const NIGHTLY_KEY: &str = "UTC@2026-05-01T02:00";

#[tokio::test]
async fn a_recorded_occurrence_is_not_handed_to_the_executor_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultyRecordStore::new(dir.path()));
    let executor = Arc::new(StubExecutor::default());
    let mut first = scheduler(store.clone(), executor.clone(), utc(2026, 5, 1, 1, 59));
    let fired = first.evaluate(utc(2026, 5, 1, 2, 0)).await;
    assert!(matches!(fired[0].outcome, Outcome::Fired { .. }));
    assert!(
        store
            .schedule_run_recorded("nightly", NIGHTLY_KEY)
            .await
            .unwrap(),
        "starting the run must record it in the claim"
    );

    // A restarted replica inside grace. The stub would start a second run if
    // it were asked to, which is what a store without the run record does.
    let mut restarted = scheduler(store, executor.clone(), utc(2026, 5, 1, 2, 3));
    let decisions = restarted.evaluate(utc(2026, 5, 1, 2, 3)).await;

    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].outcome, Outcome::NotClaimed);
    assert_eq!(
        executor.runs.load(Ordering::SeqCst),
        1,
        "the flow must not even be loaded for a recorded occurrence"
    );
}

#[tokio::test]
async fn an_unreadable_occurrence_record_is_retried_instead_of_started() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultyRecordStore::new(dir.path()));
    // A peer claimed the instant; whether it created the run is unknown
    // while the record cannot be read.
    assert!(
        store
            .claim_schedule("nightly", NIGHTLY_KEY, 604_800)
            .await
            .unwrap()
    );
    store.failing_reads.store(1, Ordering::SeqCst);
    let executor = Arc::new(StubExecutor::default());
    let mut scheduler = scheduler(store, executor.clone(), utc(2026, 5, 1, 1, 59));

    let first = scheduler.evaluate(utc(2026, 5, 1, 2, 0)).await;
    assert!(
        matches!(first[0].outcome, Outcome::ClaimFailed { .. }),
        "got {:?}",
        first[0].outcome
    );
    assert_eq!(executor.runs.load(Ordering::SeqCst), 0);

    // The read works again: the claim is an orphan and is recovered.
    let second = scheduler.evaluate(utc(2026, 5, 1, 2, 1)).await;
    assert_eq!(second.len(), 1);
    assert!(matches!(second[0].outcome, Outcome::Fired { .. }));
    assert_eq!(executor.runs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_failed_record_write_does_not_fail_the_started_run() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultyRecordStore::new(dir.path()));
    store.fail_writes.store(true, Ordering::SeqCst);
    let executor = Arc::new(StubExecutor::default());
    let mut scheduler = scheduler(store.clone(), executor.clone(), utc(2026, 5, 1, 1, 59));

    let decisions = scheduler.evaluate(utc(2026, 5, 1, 2, 0)).await;

    assert!(matches!(decisions[0].outcome, Outcome::Fired { .. }));
    assert!(
        !store
            .schedule_run_recorded("nightly", NIGHTLY_KEY)
            .await
            .unwrap()
    );

    // A peer that converges on the existing run writes the missing record.
    store.fail_writes.store(false, Ordering::SeqCst);
    executor.existing.store(true, Ordering::SeqCst);
    let mut peer = scheduler_for(store.clone(), executor.clone());
    let converged = peer.evaluate(utc(2026, 5, 1, 2, 1)).await;
    assert_eq!(converged[0].outcome, Outcome::NotClaimed);
    assert!(
        store
            .schedule_run_recorded("nightly", NIGHTLY_KEY)
            .await
            .unwrap()
    );
}

fn scheduler_for(store: Arc<FaultyRecordStore>, executor: Arc<StubExecutor>) -> Scheduler {
    Scheduler::new(daily_at_two(), store, executor, utc(2026, 5, 1, 2, 1))
}
