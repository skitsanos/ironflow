use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::{TimeZone as _, Utc};
use ironflow::engine::types::{Context, RunInfo, RunStatus, TaskState};
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::{Outcome, ScheduleExecutor, ScheduleRun, Scheduler};
use ironflow::storage::null_store::NullStateStore;
use ironflow::storage::{RunListQuery, RunSummaryPage, StateStore, StorageResult};

const HUNG: &str = "a_hung";
const HEALTHY: &str = "b_healthy";

fn schedules() -> HashMap<String, ScheduleConfig> {
    [HUNG, HEALTHY]
        .into_iter()
        .map(|name| {
            (
                name.to_string(),
                ScheduleConfig::new(
                    format!("{name}.lua"),
                    "0 2 * * *",
                    Some("UTC"),
                    None,
                    Context::new(),
                )
                .unwrap(),
            )
        })
        .collect()
}

fn start() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 1, 1, 59, 0).unwrap()
}

fn due() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 1, 2, 0, 0).unwrap()
}

#[derive(Default)]
struct RecordingExecutor {
    runs: std::sync::Mutex<Vec<String>>,
    hang_overlap_for: Option<&'static str>,
    /// How many starts of `HUNG` never return before one succeeds.
    hanging_starts: AtomicUsize,
    start_calls: AtomicUsize,
}

#[async_trait::async_trait]
impl ScheduleExecutor for RecordingExecutor {
    async fn active_run(&self, schedule_name: &str) -> Option<String> {
        if self.hang_overlap_for == Some(schedule_name) {
            std::future::pending().await
        } else {
            None
        }
    }

    fn has_capacity(&self) -> bool {
        true
    }

    async fn run(
        &self,
        schedule_name: &str,
        _instant_key: &str,
        _schedule: &ScheduleConfig,
    ) -> Result<ScheduleRun, String> {
        self.start_calls.fetch_add(1, Ordering::SeqCst);
        let hang = schedule_name == HUNG
            && self
                .hanging_starts
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                })
                .is_ok();
        if hang {
            // The load-permit wait and the parse: nothing has started yet.
            std::future::pending::<()>().await;
        }
        self.runs.lock().unwrap().push(schedule_name.to_string());
        Ok(ScheduleRun::Started {
            run_id: format!("run-{schedule_name}"),
        })
    }
}

#[derive(Default)]
struct SelectiveHangingClaimStore {
    claim_calls: AtomicUsize,
    /// Hang only on claims of `HUNG` whose key sorts before this one.
    hang_before: std::sync::Mutex<Option<&'static str>>,
    hang_disabled: std::sync::atomic::AtomicBool,
    claimed: std::sync::Mutex<std::collections::HashSet<String>>,
    recorded: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl SelectiveHangingClaimStore {
    fn unexpected<T>() -> T {
        panic!("scheduler availability fixture called an unrelated store operation")
    }
}

#[async_trait::async_trait]
impl StateStore for SelectiveHangingClaimStore {
    async fn init_run(&self, _run_id: &str, _flow_name: &str, _ctx: &Context) -> StorageResult<()> {
        Self::unexpected()
    }

    async fn set_run_status(&self, _run_id: &str, _status: RunStatus) -> StorageResult<()> {
        Self::unexpected()
    }

    async fn upsert_task(&self, _run_id: &str, _task: &TaskState) -> StorageResult<()> {
        Self::unexpected()
    }

    async fn get_ctx(&self, _run_id: &str) -> StorageResult<Context> {
        Self::unexpected()
    }

    async fn update_ctx(&self, _run_id: &str, _ctx: &Context) -> StorageResult<()> {
        Self::unexpected()
    }

    async fn get_run_info(&self, _run_id: &str) -> StorageResult<RunInfo> {
        Self::unexpected()
    }

    async fn list_runs(&self, _status: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        Self::unexpected()
    }

    async fn list_run_summaries_page(
        &self,
        _query: &RunListQuery,
    ) -> StorageResult<RunSummaryPage> {
        Self::unexpected()
    }

    async fn delete_run(&self, _run_id: &str) -> StorageResult<()> {
        Self::unexpected()
    }

    async fn claim_schedule(
        &self,
        name: &str,
        key: &str,
        _ttl_seconds: u64,
    ) -> StorageResult<bool> {
        self.claim_calls.fetch_add(1, Ordering::SeqCst);
        let selected = self
            .hang_before
            .lock()
            .unwrap()
            .is_none_or(|before| key < before);
        if name == HUNG && selected && !self.hang_disabled.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        Ok(self.claimed.lock().unwrap().insert(format!("{name}/{key}")))
    }

    async fn record_schedule_run(&self, name: &str, key: &str, _ttl: u64) -> StorageResult<()> {
        self.recorded
            .lock()
            .unwrap()
            .insert(format!("{name}/{key}"));
        Ok(())
    }

    async fn schedule_run_recorded(&self, name: &str, key: &str) -> StorageResult<bool> {
        Ok(self
            .recorded
            .lock()
            .unwrap()
            .contains(&format!("{name}/{key}")))
    }
}

const BUDGET: Duration = Duration::from_secs(15);

fn outcome_of<'a>(decisions: &'a [ironflow::scheduler::Decision], name: &str) -> &'a Outcome {
    let found = decisions.iter().find(|decision| decision.schedule == name);
    &found
        .unwrap_or_else(|| panic!("no decision for '{name}': {decisions:?}"))
        .outcome
}

fn assert_deferred_for_the_budget(decisions: &[ironflow::scheduler::Decision]) {
    assert!(
        matches!(
            outcome_of(decisions, HUNG),
            Outcome::Deferred { reason } if reason.contains("budget")
        ),
        "an expiry before the run started must keep the instant due: {decisions:?}"
    );
}

fn tick(number: i64) -> chrono::DateTime<Utc> {
    due() + chrono::Duration::seconds(30 * number)
}

// Every test below runs on the paused test clock: the budget expires because
// the fixture never answers, never because the machine is slow.

/// IF-217 (d): a permit wait and a slow parse that together reach the budget
/// expire it before any run exists. The instant stays due and starts on the
/// next tick; another schedule is not held up.
#[tokio::test(start_paused = true)]
async fn a_budget_expiry_before_the_run_starts_keeps_the_instant_due() {
    let executor = Arc::new(RecordingExecutor {
        hanging_starts: AtomicUsize::new(1),
        ..RecordingExecutor::default()
    });
    let mut scheduler = Scheduler::new(
        schedules(),
        Arc::new(NullStateStore::new()),
        executor.clone(),
        start(),
    )
    .with_evaluation_timeout(BUDGET)
    .unwrap();

    let first = scheduler.evaluate(tick(0)).await;
    assert_deferred_for_the_budget(&first);
    assert!(matches!(outcome_of(&first, HEALTHY), Outcome::Fired { .. }));
    assert_eq!(executor.runs.lock().unwrap().as_slice(), [HEALTHY]);

    let second = scheduler.evaluate(tick(1)).await;
    assert_eq!(second.len(), 1, "{second:?}");
    assert!(
        matches!(outcome_of(&second, HUNG), Outcome::Fired { .. }),
        "{second:?}"
    );
    assert_eq!(executor.runs.lock().unwrap().as_slice(), [HEALTHY, HUNG]);
    assert!(scheduler.evaluate(tick(2)).await.is_empty());
}

/// The bound: a schedule whose evaluation never fits its budget is deferred
/// twice and then burned as timed out, exactly as every expiry used to be.
#[tokio::test(start_paused = true)]
async fn a_schedule_that_always_exceeds_its_budget_is_burned_on_the_third_expiry() {
    let executor = Arc::new(RecordingExecutor {
        hanging_starts: AtomicUsize::new(usize::MAX),
        ..RecordingExecutor::default()
    });
    let mut scheduler = Scheduler::new(
        schedules(),
        Arc::new(NullStateStore::new()),
        executor.clone(),
        start(),
    )
    .with_evaluation_timeout(BUDGET)
    .unwrap();

    assert_deferred_for_the_budget(&scheduler.evaluate(tick(0)).await);
    assert_deferred_for_the_budget(&scheduler.evaluate(tick(1)).await);
    let third = scheduler.evaluate(tick(2)).await;
    assert_eq!(outcome_of(&third, HUNG), &Outcome::TimedOut, "{third:?}");

    let attempts = executor.start_calls.load(Ordering::SeqCst);
    let fourth = scheduler.evaluate(tick(3)).await;
    assert!(
        fourth.is_empty(),
        "a burned instant is not retried: {fourth:?}"
    );
    assert_eq!(executor.start_calls.load(Ordering::SeqCst), attempts);
    assert_eq!(executor.runs.lock().unwrap().as_slice(), [HEALTHY]);
}

/// The count is of consecutive expiries: an evaluation that completes resets
/// it, so a later instant of the same schedule gets its own three attempts.
#[tokio::test(start_paused = true)]
async fn a_completed_evaluation_resets_the_expiry_count() {
    let executor = Arc::new(RecordingExecutor {
        hanging_starts: AtomicUsize::new(2),
        ..RecordingExecutor::default()
    });
    let mut scheduler = Scheduler::new(
        schedules(),
        Arc::new(NullStateStore::new()),
        executor.clone(),
        start(),
    )
    .with_evaluation_timeout(BUDGET)
    .unwrap();

    assert_deferred_for_the_budget(&scheduler.evaluate(tick(0)).await);
    assert_deferred_for_the_budget(&scheduler.evaluate(tick(1)).await);
    let third = scheduler.evaluate(tick(2)).await;
    assert!(matches!(outcome_of(&third, HUNG), Outcome::Fired { .. }));

    // The next day's instant hangs again and is deferred, not burned at once.
    executor.hanging_starts.store(1, Ordering::SeqCst);
    let next_day = due() + chrono::Duration::days(1);
    assert_deferred_for_the_budget(&scheduler.evaluate(next_day).await);
}

#[tokio::test(start_paused = true)]
async fn a_hanging_claim_is_bounded_and_does_not_block_another_schedule() {
    let store = Arc::new(SelectiveHangingClaimStore::default());
    let executor = Arc::new(RecordingExecutor::default());
    let mut scheduler = Scheduler::new(schedules(), store.clone(), executor.clone(), start())
        .with_evaluation_timeout(BUDGET)
        .unwrap();

    let started = tokio::time::Instant::now();
    let decisions = scheduler.evaluate(tick(0)).await;
    assert_eq!(started.elapsed(), BUDGET, "the hung name costs one budget");
    assert_deferred_for_the_budget(&decisions);
    assert!(matches!(
        outcome_of(&decisions, HEALTHY),
        Outcome::Fired { .. }
    ));
    assert_eq!(executor.runs.lock().unwrap().as_slice(), [HEALTHY]);

    // The claim keeps hanging: two more evaluations, then the instant is
    // burned and the store is not asked again.
    assert_deferred_for_the_budget(&scheduler.evaluate(tick(1)).await);
    let third = scheduler.evaluate(tick(2)).await;
    assert_eq!(outcome_of(&third, HUNG), &Outcome::TimedOut, "{third:?}");
    let calls_after_burn = store.claim_calls.load(Ordering::SeqCst);
    let next = scheduler.evaluate(tick(4)).await;
    assert!(next.is_empty(), "a burned instant was retried: {next:?}");
    assert_eq!(store.claim_calls.load(Ordering::SeqCst), calls_after_burn);
    assert_eq!(executor.runs.lock().unwrap().as_slice(), [HEALTHY]);
}

#[tokio::test(start_paused = true)]
async fn a_hanging_overlap_read_does_not_block_another_schedule() {
    let executor = Arc::new(RecordingExecutor {
        hang_overlap_for: Some(HUNG),
        ..RecordingExecutor::default()
    });
    let mut scheduler = Scheduler::new(
        schedules(),
        Arc::new(NullStateStore::new()),
        executor.clone(),
        start(),
    )
    .with_evaluation_timeout(BUDGET)
    .unwrap();

    let decisions = scheduler.evaluate(tick(0)).await;
    assert_deferred_for_the_budget(&decisions);
    assert!(matches!(
        outcome_of(&decisions, HEALTHY),
        Outcome::Fired { .. }
    ));
    assert_eq!(executor.runs.lock().unwrap().as_slice(), [HEALTHY]);
}

/// An expiry keeps what the evaluation had already decided: the run that
/// started is reported as fired, and the missed instants it had not yet
/// settled are settled on the next tick, with no second run.
#[tokio::test(start_paused = true)]
async fn an_expiry_after_the_run_started_keeps_the_fired_decision() {
    const LATEST: &str = "UTC@2026-05-01T02:00";
    let store = Arc::new(SelectiveHangingClaimStore::default());
    *store.hang_before.lock().unwrap() = Some(LATEST);
    let executor = Arc::new(RecordingExecutor::default());
    let every_minute =
        ScheduleConfig::new("f.lua", "* * * * *", Some("UTC"), None, Context::new()).unwrap();
    // Started ten seconds after 02:00 with the default 300 seconds of grace:
    // 01:56 to 01:59 are missed, 02:00 is the most recent instant.
    let now = due() + chrono::Duration::seconds(10);
    let mut scheduler = Scheduler::new(
        HashMap::from([(HUNG.to_string(), every_minute)]),
        store.clone(),
        executor.clone(),
        now,
    )
    .with_evaluation_timeout(BUDGET)
    .unwrap();

    let first = scheduler.evaluate(now).await;
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0].key, LATEST);
    assert!(
        matches!(first[0].outcome, Outcome::Fired { .. }),
        "{first:?}"
    );

    store.hang_disabled.store(true, Ordering::SeqCst);
    let second = scheduler
        .evaluate(now + chrono::Duration::seconds(30))
        .await;
    let outcomes = second
        .iter()
        .map(|decision| (decision.key.as_str(), &decision.outcome))
        .collect::<Vec<_>>();
    let superseded = Outcome::Superseded {
        by: LATEST.to_string(),
    };
    assert_eq!(
        outcomes,
        [
            ("UTC@2026-05-01T01:56", &superseded),
            ("UTC@2026-05-01T01:57", &superseded),
            ("UTC@2026-05-01T01:58", &superseded),
            ("UTC@2026-05-01T01:59", &superseded),
            (LATEST, &Outcome::NotClaimed),
        ]
    );
    assert_eq!(executor.runs.lock().unwrap().as_slice(), [HUNG]);
}

#[test]
fn a_zero_evaluation_budget_is_rejected_without_panicking() {
    let result = Scheduler::new(
        schedules(),
        Arc::new(NullStateStore::new()),
        Arc::new(RecordingExecutor::default()),
        start(),
    )
    .with_evaluation_timeout(Duration::ZERO);

    assert!(result.is_err());
}
