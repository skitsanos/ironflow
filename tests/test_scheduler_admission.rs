// The admission cap is the highest-risk property of the detached-run change
// (Finding 2 elsewhere in this branch: `FlowExecutor::run` starts a run and
// returns without awaiting it). If the run-admission permit were dropped
// when `run()` returns instead of moving into the spawned task,
// `IRONFLOW_MAX_CONCURRENT_RUNS` would stop bounding anything and every
// other test on this branch would still pass.
//
// API admission control caches the configured cap behind a
// process-wide `OnceLock`, so the env var must be set before anything in
// this process calls it — own test binary, same pattern as
// test_limits_defaults.rs and test_conversion_limits_env.rs.

use ironflow::engine::types::{Context, RunStatus};
use ironflow::scheduler::ScheduleExecutor;
use ironflow::scheduler::config::ScheduleConfig;

#[path = "support/scheduler.rs"]
mod scheduler_support;

use scheduler_support::{build_controlled_executor, wait_for_terminal};

/// Every test in this binary shares the one run permit, so they serialize.
static ONE_RUN_PERMIT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Cap the process at one concurrent run. Must happen before anything in the
/// process reaches admission control: `has_capacity`/`run` go through
/// `crate::api::acquire_run_permit`, whose `OnceLock` reads this variable on
/// its first call only. Every test calls this first, under `ONE_RUN_PERMIT`.
fn cap_runs_at_one() {
    static CAPPED: std::sync::Once = std::sync::Once::new();
    CAPPED.call_once(|| unsafe { std::env::set_var("IRONFLOW_MAX_CONCURRENT_RUNS", "1") });
}

/// A flow whose only step is the test-controlled `delay` node.
fn slow_flows() -> tempfile::TempDir {
    let flows = tempfile::tempdir().unwrap();
    std::fs::write(
        flows.path().join("slow.lua"),
        r#"
        local flow = Flow.new("slow_flow")
        flow:step("wait", nodes.delay({ seconds = 2 }))
        return flow
        "#,
    )
    .unwrap();
    flows
}

async fn wait_for_capacity(executor: &ironflow::scheduler::execution::FlowExecutor) {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while !executor.has_capacity() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("capacity was not restored after supervised run completion");
}

#[tokio::test]
async fn the_admission_permit_is_held_for_the_runs_real_duration() {
    let _serial = ONE_RUN_PERMIT.lock().await;
    cap_runs_at_one();

    let flows = slow_flows();

    let (app, delay) = build_controlled_executor(flows.path());
    let schedule =
        ScheduleConfig::new("slow.lua", "0 2 * * *", Some("UTC"), None, Context::new()).unwrap();

    wait_for_capacity(&app.executor).await;

    let run_id = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        app.executor.run("slow", &schedule),
    )
    .await
    .expect("run() waited for task completion")
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), delay.started.notified())
        .await
        .expect("detached task did not start");

    // The assertion that matters: this can only hold if the permit acquired
    // inside `run()` moved into the spawned task rather than being dropped
    // when `run()` returned. If the permit were mishandled — dropped early,
    // or never held past `run()` — this would read `true` here and the cap
    // would not be bounding concurrent runs at all.
    assert!(
        !app.executor.has_capacity(),
        "the permit must still be held while the run is in flight"
    );

    delay.release.notify_one();
    let status = wait_for_terminal(&app.store, &run_id).await;
    assert_eq!(status, RunStatus::Success);

    // The terminal status write precedes the final event and heartbeat
    // shutdown. Admission deliberately covers that complete supervised
    // lifecycle, so wait for the detached waiter to settle instead of treating
    // the first observable terminal read as the permit-release instant.
    wait_for_capacity(&app.executor).await;
}

/// The real executor behind a capacity probe that always answers "free": the
/// state a tick is in when the run cap fills between `has_capacity` and the
/// permit acquire inside `run` (IF-217 a).
struct ProbeSawCapacity(std::sync::Arc<ironflow::scheduler::execution::FlowExecutor>);

#[async_trait::async_trait]
impl ScheduleExecutor for ProbeSawCapacity {
    async fn active_run(&self, schedule_name: &str) -> Option<String> {
        self.0.active_run(schedule_name).await
    }

    fn has_capacity(&self) -> bool {
        true
    }

    async fn run(
        &self,
        schedule_name: &str,
        instant_key: &str,
        schedule: &ScheduleConfig,
    ) -> Result<ironflow::scheduler::ScheduleRun, String> {
        ScheduleExecutor::run(self.0.as_ref(), schedule_name, instant_key, schedule).await
    }
}

#[tokio::test]
async fn a_run_cap_that_fills_after_the_probe_is_reported_as_at_capacity() {
    use chrono::TimeZone as _;
    use ironflow::scheduler::{Outcome, Scheduler};
    use ironflow::storage::StateStore as _;

    let _serial = ONE_RUN_PERMIT.lock().await;
    cap_runs_at_one();

    let flows = slow_flows();
    let (app, delay) = build_controlled_executor(flows.path());
    let schedule =
        ScheduleConfig::new("slow.lua", "0 2 * * *", Some("UTC"), None, Context::new()).unwrap();

    // Another trigger takes the only run permit.
    let holder = app.executor.run("holder", &schedule).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), delay.started.notified())
        .await
        .expect("the holding run did not start");

    let now = chrono::Utc.with_ymd_and_hms(2026, 3, 10, 2, 0, 5).unwrap();
    let mut scheduler = Scheduler::new(
        [("squeezed".to_string(), schedule)].into(),
        app.store.clone(),
        std::sync::Arc::new(ProbeSawCapacity(app.executor.clone())),
        now,
    );

    let decisions = scheduler.evaluate(now).await;

    assert_eq!(decisions.len(), 1, "{decisions:?}");
    assert_eq!(decisions[0].outcome, Outcome::AtCapacity, "{decisions:?}");
    assert_eq!(
        app.store.list_runs(None).await.unwrap().len(),
        1,
        "only the holding run exists"
    );

    delay.release.notify_one();
    assert_eq!(
        wait_for_terminal(&app.store, &holder).await,
        RunStatus::Success
    );
    wait_for_capacity(&app.executor).await;
}
