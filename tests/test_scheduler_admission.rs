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

use scheduler_support::{build_executor, wait_for_terminal};

#[tokio::test]
async fn the_admission_permit_is_held_for_the_runs_real_duration() {
    // Must be the first thing this process does: `has_capacity`/`run` reach
    // admission control through `crate::api::acquire_run_permit`, and that
    // `OnceLock` only reads this variable on its first call.
    unsafe { std::env::set_var("IRONFLOW_MAX_CONCURRENT_RUNS", "1") };

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

    let app = build_executor(flows.path());
    let schedule =
        ScheduleConfig::new("slow.lua", "0 2 * * *", Some("UTC"), None, Context::new()).unwrap();

    assert!(
        app.executor.has_capacity(),
        "capacity should be available before any run starts"
    );

    let started = std::time::Instant::now();
    let run_id = app.executor.run("slow", &schedule).await.unwrap();
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(1500),
        "run() blocked for {elapsed:?}; it must return once the run has started, \
         not once the 2-second delay step has finished"
    );

    // The assertion that matters: this can only hold if the permit acquired
    // inside `run()` moved into the spawned task rather than being dropped
    // when `run()` returned. If the permit were mishandled — dropped early,
    // or never held past `run()` — this would read `true` here and the cap
    // would not be bounding concurrent runs at all.
    assert!(
        !app.executor.has_capacity(),
        "the permit must still be held while the run is in flight"
    );

    let status = wait_for_terminal(&app.store, &run_id).await;
    assert_eq!(status, RunStatus::Success);

    // The terminal status write precedes the final event and heartbeat
    // shutdown. Admission deliberately covers that complete supervised
    // lifecycle, so wait for the detached waiter to settle instead of treating
    // the first observable terminal read as the permit-release instant.
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if app.executor.has_capacity() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("capacity was not restored after supervised run completion");
}
