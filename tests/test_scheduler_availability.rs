use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

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
        self.runs.lock().unwrap().push(schedule_name.to_string());
        Ok(ScheduleRun::Started {
            run_id: format!("run-{schedule_name}"),
        })
    }
}

struct SelectiveHangingClaimStore {
    claim_calls: AtomicUsize,
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
        _key: &str,
        _ttl_seconds: u64,
    ) -> StorageResult<bool> {
        self.claim_calls.fetch_add(1, Ordering::SeqCst);
        if name == HUNG {
            std::future::pending().await
        } else {
            Ok(true)
        }
    }
}

fn assert_isolated(decisions: &[ironflow::scheduler::Decision], executor: &RecordingExecutor) {
    assert!(
        decisions
            .iter()
            .any(|decision| decision.schedule == HUNG && decision.outcome == Outcome::TimedOut),
        "hung schedule was not surfaced as indeterminate: {decisions:?}"
    );
    assert!(
        decisions.iter().any(|decision| {
            decision.schedule == HEALTHY && matches!(decision.outcome, Outcome::Fired { .. })
        }),
        "healthy schedule did not fire independently: {decisions:?}"
    );
    assert_eq!(executor.runs.lock().unwrap().as_slice(), [HEALTHY]);
}

#[tokio::test]
async fn a_hanging_claim_is_bounded_burned_and_does_not_block_another_schedule() {
    let store = Arc::new(SelectiveHangingClaimStore {
        claim_calls: AtomicUsize::new(0),
    });
    let executor = Arc::new(RecordingExecutor::default());
    let mut scheduler = Scheduler::new(schedules(), store.clone(), executor.clone(), start())
        .with_evaluation_timeout(Duration::from_millis(40))
        .unwrap();

    let started = Instant::now();
    let decisions = scheduler.evaluate(due()).await;
    assert!(started.elapsed() < Duration::from_millis(250));
    assert_isolated(&decisions, &executor);

    let calls_after_timeout = store.claim_calls.load(Ordering::SeqCst);
    let next = scheduler
        .evaluate(due() + chrono::Duration::minutes(1))
        .await;
    assert!(next.is_empty(), "timed-out instant was retried: {next:?}");
    assert_eq!(
        store.claim_calls.load(Ordering::SeqCst),
        calls_after_timeout,
        "an indeterminate claim must be burned, not retried"
    );
}

#[tokio::test]
async fn a_hanging_overlap_read_does_not_block_another_schedule() {
    let executor = Arc::new(RecordingExecutor {
        runs: std::sync::Mutex::new(Vec::new()),
        hang_overlap_for: Some(HUNG),
    });
    let mut scheduler = Scheduler::new(
        schedules(),
        Arc::new(NullStateStore::new()),
        executor.clone(),
        start(),
    )
    .with_evaluation_timeout(Duration::from_millis(40))
    .unwrap();

    let decisions = scheduler.evaluate(due()).await;
    assert_isolated(&decisions, &executor);
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
