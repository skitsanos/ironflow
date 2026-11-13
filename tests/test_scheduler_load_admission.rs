//! IF-182: flow-load admission must delay a scheduled instant, never consume it.
//!
//! Every test here drives the real `FlowExecutor` against the process-wide
//! flow-load semaphore at its default of two permits, so the tests serialize
//! on `PERMITS` instead of racing each other for those two slots.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use chrono::{DateTime, TimeZone as _, Utc};
use ironflow::api::{AppState, handlers};
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::execution::FlowExecutor;
use ironflow::scheduler::{Decision, Outcome, Scheduler};
use ironflow::storage::StateStore as _;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::util::listing::ListingPolicy;
use tower::ServiceExt as _;

// The end-to-end tests in this binary start real scheduled runs with short
// time bounds, so they must never overlap a test here that holds the permits.
use super::test_scheduler_e2e::PROCESS_ADMISSION_LOCK as PERMITS;

/// Five seconds after the 02:00 UTC occurrence every schedule here matches.
fn just_after_two() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 10, 2, 0, 5).unwrap()
}

/// A flow definition whose top-level chunk keeps its load permit for about
/// `hold_ms`. Each iteration does real work in one VM instruction, so the
/// loop stays far below the default Lua instruction budget.
fn slow_flow(name: &str, hold_ms: u64) -> String {
    format!(
        r#"
        local stop_at = now_unix_ms() + {hold_ms}
        while now_unix_ms() < stop_at do string.rep("x", 200000) end
        local flow = Flow.new("{name}")
        flow:step("emit", nodes.log({{ message = "ran" }}))
        return flow
        "#
    )
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

fn validation_router(
    registry: Arc<NodeRegistry>,
    store: Arc<JsonStateStore>,
    events: Arc<MemoryEventStore>,
    flows_dir: &Path,
) -> Router {
    let state = Arc::new(AppState {
        registry,
        store,
        event_store: events,
        flows_dir: Some(flows_dir.to_path_buf()),
        max_concurrent_tasks: Some(1),
        listing_policy: ListingPolicy::default(),
        webhooks: HashMap::new(),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
        metrics: None,
    });
    Router::new()
        .route("/flows/validate", post(handlers::validate_flow))
        .with_state(state)
}

async fn validate(router: &Router, source: String) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri("/flows/validate")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "source": source }).to_string(),
        ))
        .unwrap();
    router.clone().oneshot(request).await.unwrap().status()
}

/// Occupy both flow-load permits with `/flows/validate` parses for about
/// `hold_ms`. A refused request returns at once, so a request still pending
/// after 50 milliseconds owns a permit; other tests in this binary can take a
/// permit for a moment, which is why a refused holder is simply resubmitted.
/// The final refusal is evidence that API admission stays fail-fast.
async fn hold_both_load_permits(router: &Router, hold_ms: u64) {
    for index in 0..2 {
        let source = slow_flow(&format!("holder-{index}"), hold_ms);
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let (router, source) = (router.clone(), source.clone());
                let holder = tokio::spawn(async move { validate(&router, source).await });
                tokio::time::sleep(Duration::from_millis(50)).await;
                if !holder.is_finished() {
                    return;
                }
            }
        })
        .await
        .expect("a slow parse never obtained a flow-load permit");
    }
    let probe = "local flow = Flow.new('probe')\nreturn flow".to_string();
    assert_eq!(
        validate(router, probe).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
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
        .evaluate(now + chrono::Duration::seconds(30))
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
    let retry_at = now + chrono::Duration::seconds(90);
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
        .evaluate(retry_at + chrono::Duration::seconds(30))
        .await;
    assert!(third.is_empty(), "{third:?}");
    assert_eq!(run_count(&fixture.store).await, 1);
}
