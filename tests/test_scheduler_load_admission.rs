//! IF-182: flow-load admission must delay a scheduled instant, never consume it.
//!
//! Every test here drives the real `FlowExecutor` against the process-wide
//! flow-load semaphore at its default of two permits, so the tests serialize
//! on `PERMITS` instead of racing each other for those two slots.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::execution::FlowExecutor;
use ironflow::scheduler::{Decision, Outcome, Scheduler};
use ironflow::storage::StateStore as _;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use jiff::{SignedDuration, Timestamp};

#[path = "support/flow_load.rs"]
mod flow_load;

use flow_load::{hold_both_load_permits, slow_flow, validate, validation_router};

// The end-to-end tests in this binary start real scheduled runs with short
// time bounds, so they must never overlap a test here that holds the permits.
use super::test_scheduler_e2e::PROCESS_ADMISSION_LOCK as PERMITS;

/// Five seconds after the 02:00 UTC occurrence every schedule here matches.
fn utc_at(year: i16, month: i8, day: i8, hour: i8, minute: i8, second: i8) -> Timestamp {
    let wall_clock = jiff::civil::datetime(year, month, day, hour, minute, second, 0);
    jiff::tz::Offset::UTC.to_timestamp(wall_clock).unwrap()
}

fn just_after_two() -> Timestamp {
    utc_at(2026, 3, 10, 2, 0, 5)
}

struct Fixture {
    executor: Arc<FlowExecutor>,
    store: Arc<JsonStateStore>,
    router: Router,
    _flows: tempfile::TempDir,
    _store_dir: tempfile::TempDir,
}

fn fixture(configure: impl FnOnce(FlowExecutor) -> FlowExecutor) -> Fixture {
    let flows = tempfile::tempdir().unwrap();
    std::fs::write(flows.path().join("job.lua"), slow_flow("job", 100)).unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(store_dir.path()));
    let events = Arc::new(MemoryEventStore::new());
    let registry = Arc::new(NodeRegistry::with_builtins());
    let executor = Arc::new(configure(FlowExecutor::new(
        registry.clone(),
        store.clone(),
        events.clone(),
        Some(flows.path().to_path_buf()),
        None,
    )));
    let router = validation_router(registry, store.clone(), events, flows.path());
    Fixture {
        executor,
        store,
        router,
        _flows: flows,
        _store_dir: store_dir,
    }
}

fn schedules(names: &[&str], grace_seconds: Option<u64>) -> HashMap<String, ScheduleConfig> {
    names
        .iter()
        .map(|name| {
            let schedule = ScheduleConfig::new(
                "job.lua",
                "0 2 * * *",
                Some("UTC"),
                grace_seconds,
                Context::new(),
            )
            .unwrap();
            ((*name).to_string(), schedule)
        })
        .collect()
}

fn fired_run_ids(decisions: &[Decision]) -> Vec<String> {
    decisions
        .iter()
        .map(|decision| match &decision.outcome {
            Outcome::Fired { run_id } => run_id.clone(),
            other => panic!(
                "schedule '{}' did not fire at {}: {other:?}",
                decision.schedule, decision.key
            ),
        })
        .collect()
}

async fn run_count(store: &JsonStateStore) -> usize {
    store.list_runs(None).await.unwrap().len()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn eight_schedules_due_together_each_run_once_with_two_load_permits() {
    let _serial = PERMITS.lock().await;
    let fixture = fixture(|executor| executor);
    let names = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let now = just_after_two();
    let mut scheduler = Scheduler::new(
        schedules(&names, None),
        fixture.store.clone(),
        fixture.executor.clone(),
        now,
    );

    let decisions = scheduler.evaluate(now).await;

    assert_eq!(decisions.len(), 8);
    let mut run_ids = fired_run_ids(&decisions);
    run_ids.sort();
    run_ids.dedup();
    assert_eq!(run_ids.len(), 8, "every schedule owns a distinct run");
    assert_eq!(run_count(&fixture.store).await, 8);

    // The instants are settled: a later tick neither repeats nor adds a run.
    let later = scheduler
        .evaluate(now + SignedDuration::from_secs(30))
        .await;
    assert!(later.is_empty(), "{later:?}");
    assert_eq!(run_count(&fixture.store).await, 8);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn load_permits_held_by_api_parses_delay_the_instant_within_the_tick() {
    let _serial = PERMITS.lock().await;
    let fixture = fixture(|executor| executor);
    let now = just_after_two();
    let mut scheduler = Scheduler::new(
        schedules(&["nightly"], None),
        fixture.store.clone(),
        fixture.executor.clone(),
        now,
    );
    hold_both_load_permits(&fixture.router, 600).await;

    let decisions = scheduler.evaluate(now).await;

    assert_eq!(fired_run_ids(&decisions).len(), 1);
    assert_eq!(run_count(&fixture.store).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_expired_load_wait_defers_the_instant_and_grace_bounds_the_retry() {
    let _serial = PERMITS.lock().await;
    // A wait far shorter than the held parses forces the deferral path.
    let fixture = fixture(|executor| executor.with_flow_load_wait(Duration::from_millis(50)));
    let now = just_after_two();
    let mut configured = schedules(&["patient"], Some(300));
    configured.extend(schedules(&["strict"], Some(60)));
    let mut scheduler = Scheduler::new(
        configured,
        fixture.store.clone(),
        fixture.executor.clone(),
        now,
    );
    // The hold is wall-clock time, so it must outlast everything up to the
    // first evaluation even on a heavily loaded machine: at 1.5 seconds the
    // parses finished first under a parallel build and the instant fired.
    hold_both_load_permits(&fixture.router, 5_000).await;

    let first = scheduler.evaluate(now).await;

    assert_eq!(first.len(), 2);
    for decision in &first {
        assert!(
            matches!(decision.outcome, Outcome::Deferred { .. }),
            "{decision:?}"
        );
    }
    assert_eq!(run_count(&fixture.store).await, 0, "nothing ran or parsed");

    // Let the held parses settle so the retry meets a free limit.
    tokio::time::timeout(Duration::from_secs(30), async {
        let probe = || "local flow = Flow.new('probe')\nreturn flow".to_string();
        while validate(&fixture.router, probe()).await != StatusCode::OK {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the held parses never released the flow-load permits");

    // 95 seconds late: inside the 300-second grace, outside the 60-second one.
    let retry_at = now + SignedDuration::from_secs(90);
    let second = scheduler.evaluate(retry_at).await;

    let outcome = |name: &str| {
        let decision = second.iter().find(|decision| decision.schedule == name);
        decision.map(|decision| decision.outcome.clone())
    };
    assert!(
        matches!(outcome("patient"), Some(Outcome::Fired { .. })),
        "{second:?}"
    );
    assert!(
        matches!(outcome("strict"), Some(Outcome::Late { .. })),
        "{second:?}"
    );
    assert_eq!(run_count(&fixture.store).await, 1);

    let third = scheduler
        .evaluate(retry_at + SignedDuration::from_secs(30))
        .await;
    assert!(third.is_empty(), "{third:?}");
    assert_eq!(run_count(&fixture.store).await, 1);
}
