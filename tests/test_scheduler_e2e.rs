use std::sync::Arc;

use chrono::{TimeZone as _, Utc};
use ironflow::engine::types::Context;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::execution::SCHEDULE_CONTEXT_KEY;
use ironflow::scheduler::{Outcome, ScheduleExecutor, Scheduler};
use ironflow::storage::StateStore;

#[path = "support/scheduler.rs"]
mod scheduler_support;

use scheduler_support::{build_executor, flows_with_logger, wait_for_terminal};

fn nightly(flow: &str) -> ScheduleConfig {
    let mut ctx = Context::new();
    ctx.insert("region".to_string(), serde_json::json!("eu"));
    ScheduleConfig::new(flow, "0 2 * * *", Some("UTC"), None, ctx).unwrap()
}

#[tokio::test]
async fn a_scheduled_run_is_an_ordinary_persisted_run() {
    let flows = flows_with_logger();
    let app = build_executor(flows.path());

    let run_id = app
        .executor
        .run("nightly", &nightly("nightly.lua"))
        .await
        .unwrap();

    // `run()` now returns once the run has started, not once it has finished
    // (Finding 2): settle before asserting on terminal state.
    let status = wait_for_terminal(&app.store, &run_id).await;
    assert!(status.is_terminal());

    let info = app.store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.flow_name, "nightly_report");
    assert!(info.status.is_terminal());
}

#[tokio::test]
async fn the_schedule_name_is_recorded_so_a_run_traces_to_its_trigger() {
    let flows = flows_with_logger();
    let app = build_executor(flows.path());

    let run_id = app
        .executor
        .run("nightly", &nightly("nightly.lua"))
        .await
        .unwrap();

    let info = app.store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.ctx[SCHEDULE_CONTEXT_KEY], serde_json::json!("nightly"));
    assert_eq!(
        info.ctx["region"],
        serde_json::json!("eu"),
        "configured context is merged"
    );
    assert!(
        info.ctx.contains_key("_flow_dir"),
        "subworkflow resolution needs _flow_dir"
    );
}

#[tokio::test]
async fn a_flow_outside_flows_dir_is_refused() {
    let flows = flows_with_logger();
    let app = build_executor(flows.path());

    let error = app
        .executor
        .run("escape", &nightly("../../../etc/passwd"))
        .await
        .unwrap_err();
    assert!(!error.is_empty());

    // Nothing was persisted.
    assert!(app.store.list_runs(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_missing_flow_fails_the_run_not_the_scheduler() {
    let flows = flows_with_logger();
    let app = build_executor(flows.path());

    assert!(
        app.executor
            .run("gone", &nightly("absent.lua"))
            .await
            .is_err()
    );
    // The executor is still usable.
    assert!(
        app.executor
            .run("nightly", &nightly("nightly.lua"))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_flow_load_failure_redacts_credentials_before_they_reach_the_error() {
    // A Lua syntax error echoes the offending token verbatim (`near '<token>'`).
    // When that token is a credential-bearing URL, the same redaction the REST
    // API applies to this exact failure (`helpers::flow_file_load_error`) must
    // run here too, or the credential lands in the scheduler's WARN log.
    let flows = tempfile::tempdir().unwrap();
    std::fs::write(
        flows.path().join("broken.lua"),
        r#"
        local flow = Flow.new("broken")
        local "https://user:sekret-value-123@example.com/hook" = 5
        return flow
        "#,
    )
    .unwrap();
    let app = build_executor(flows.path());

    let error = app
        .executor
        .run("nightly", &nightly("broken.lua"))
        .await
        .unwrap_err();

    assert!(
        !error.contains("sekret-value-123"),
        "flow-load error must be redacted before it becomes the scheduler's Outcome::Failed string: {error}"
    );
    assert!(error.contains("[REDACTED]"), "{error}");
}

#[tokio::test]
async fn overlap_detection_finds_a_non_terminal_run_of_the_same_schedule() {
    let flows = flows_with_logger();
    let app = build_executor(flows.path());

    // No run yet.
    assert!(app.executor.active_run("nightly").await.is_none());

    // A completed run is not an overlap. `run()` only awaits the run's start
    // (Finding 2), so settle to its terminal state before checking overlap.
    let run_id = app
        .executor
        .run("nightly", &nightly("nightly.lua"))
        .await
        .unwrap();
    wait_for_terminal(&app.store, &run_id).await;
    assert!(app.executor.active_run("nightly").await.is_none());

    // A run left non-terminal is.
    let mut ctx = Context::new();
    ctx.insert(
        SCHEDULE_CONTEXT_KEY.to_string(),
        serde_json::json!("nightly"),
    );
    app.store
        .init_run("stuck-run", "nightly_report", &ctx)
        .await
        .unwrap();
    app.store
        .set_run_status("stuck-run", ironflow::engine::types::RunStatus::Running)
        .await
        .unwrap();
    assert_eq!(
        app.executor.active_run("nightly").await.as_deref(),
        Some("stuck-run")
    );

    // Another schedule's non-terminal run is not this schedule's overlap.
    assert!(app.executor.active_run("other").await.is_none());
}

#[tokio::test]
async fn a_long_running_schedule_does_not_block_the_next_evaluation() {
    // The tick loop must not await a run to completion: one slow flow would
    // otherwise starve every other schedule and, if it never returned, end all
    // scheduling silently.
    let flows = tempfile::tempdir().unwrap();
    std::fs::write(
        flows.path().join("slow.lua"),
        r#"
        local flow = Flow.new("slow_flow")
        flow:step("wait", nodes.delay({ seconds = 3 }))
        return flow
        "#,
    )
    .unwrap();

    let app = build_executor(flows.path());
    let schedule =
        ScheduleConfig::new("slow.lua", "0 2 * * *", Some("UTC"), None, Context::new()).unwrap();

    let started = std::time::Instant::now();
    let run_id = app.executor.run("slow", &schedule).await.unwrap();
    let elapsed = started.elapsed();

    assert!(
        elapsed < std::time::Duration::from_millis(1500),
        "run() blocked for {elapsed:?}; it must return once the run has started"
    );
    assert!(!run_id.is_empty());

    // The run is genuinely in flight, not skipped.
    assert_eq!(
        app.executor.active_run("slow").await.as_deref(),
        Some(run_id.as_str())
    );

    // A non-terminal record alone doesn't prove the detached task is doing
    // anything: `init_run` writes it synchronously inside `start()`, before
    // the run task is even spawned, so the assertion above would pass
    // identically if `tokio::spawn` were deleted or the task panicked on its
    // first poll. Settling to a terminal status is the only thing that can't
    // be faked that way — it proves the detached task actually carried the
    // flow through.
    let status = wait_for_terminal(&app.store, &run_id).await;
    assert_eq!(status, ironflow::engine::types::RunStatus::Success);
}

#[tokio::test]
async fn two_schedulers_sharing_one_store_fire_an_instant_exactly_once() {
    let flows = flows_with_logger();
    let store_dir = tempfile::tempdir().unwrap();

    let build = || {
        let store = Arc::new(ironflow::storage::json_store::JsonStateStore::new(
            store_dir.path(),
        ));
        let events = Arc::new(ironflow::storage::event_store::MemoryEventStore::new());
        let executor = Arc::new(ironflow::scheduler::execution::FlowExecutor::new(
            Arc::new(ironflow::nodes::NodeRegistry::with_builtins()),
            store.clone() as Arc<dyn StateStore>,
            events,
            Some(flows.path().to_path_buf()),
            None,
        ));
        let schedules =
            std::collections::HashMap::from([("nightly".to_string(), nightly("nightly.lua"))]);
        (
            Scheduler::new(
                schedules,
                store.clone() as Arc<dyn StateStore>,
                executor as Arc<dyn ScheduleExecutor>,
                Utc.with_ymd_and_hms(2026, 5, 1, 1, 59, 0).unwrap(),
            ),
            store,
        )
    };

    let (mut first, store) = build();
    let (mut second, _) = build();

    let now = Utc.with_ymd_and_hms(2026, 5, 1, 2, 0, 0).unwrap();
    let (left, right) = tokio::join!(first.evaluate(now), second.evaluate(now));

    let fired: Vec<_> = left
        .iter()
        .chain(right.iter())
        .filter(|d| matches!(d.outcome, Outcome::Fired { .. }))
        .collect();
    assert_eq!(fired.len(), 1, "each instant must produce exactly one run");

    let runs = store.list_runs(None).await.unwrap();
    assert_eq!(runs.len(), 1);
}

#[tokio::test]
async fn the_spawned_tick_loop_fires_a_due_schedule() {
    let flows = flows_with_logger();
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ironflow::storage::json_store::JsonStateStore::new(
        store_dir.path(),
    ));
    let events = Arc::new(ironflow::storage::event_store::MemoryEventStore::new());

    // Every minute, with the shortest legal grace window. `Scheduler::new`
    // seeds catch-up at `now - grace_seconds`, so the first tick evaluates
    // `(now - 60s, now]` — a half-open 60-second interval, which contains
    // exactly one whole-minute boundary wherever `now` falls. Exactly one
    // instant is therefore due, with no dependence on timing.
    //
    // The default 300-second grace would make five instants due here, which
    // is correct catch-up behaviour but not what this test is measuring.
    let mut ctx = Context::new();
    ctx.insert("region".to_string(), serde_json::json!("eu"));
    let schedule =
        ScheduleConfig::new("nightly.lua", "* * * * *", Some("UTC"), Some(60), ctx).unwrap();

    let handle = ironflow::scheduler::spawn(
        std::collections::HashMap::from([("frequent".to_string(), schedule)]),
        store.clone() as Arc<dyn StateStore>,
        events,
        Some(flows.path().to_path_buf()),
        None,
    )
    .expect("a non-empty schedule map must spawn a scheduler");

    // The loop evaluates immediately on entry, so the due instant runs without
    // waiting out a full tick.
    let mut runs = Vec::new();
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        runs = store.list_runs(None).await.unwrap();
        if !runs.is_empty() {
            break;
        }
    }
    assert!(
        !runs.is_empty(),
        "the tick loop did not fire a due schedule"
    );

    // Settle, then confirm the count is stable: one instant was due, so one
    // run fired and nothing followed it. Snapshotting the moment the first run
    // appears would pass even if more were still landing.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let runs = store.list_runs(None).await.unwrap();
    handle.abort();

    assert_eq!(
        runs.len(),
        1,
        "expected exactly one due instant, got {runs:?}"
    );
    assert_eq!(runs[0].flow_name, "nightly_report");
    assert_eq!(
        runs[0].ctx[SCHEDULE_CONTEXT_KEY],
        serde_json::json!("frequent")
    );
}

#[tokio::test]
async fn no_schedules_means_no_scheduler_task() {
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ironflow::storage::json_store::JsonStateStore::new(
        store_dir.path(),
    ));
    let events = Arc::new(ironflow::storage::event_store::MemoryEventStore::new());

    assert!(
        ironflow::scheduler::spawn(
            std::collections::HashMap::new(),
            store as Arc<dyn StateStore>,
            events,
            None,
            None,
        )
        .is_none()
    );
}

#[test]
fn a_schedule_naming_a_missing_flow_is_rejected_at_startup() {
    // `flow: reports/nightl.lua` (a typo) must fail `serve` at startup rather
    // than being first reported as a WARN at the schedule's next due instant.
    // This calls the validation `serve.rs` runs directly, without booting a
    // server.
    let flows_dir = tempfile::tempdir().unwrap();
    let schedules =
        std::collections::HashMap::from([("nightly".to_string(), nightly("reports/nightl.lua"))]);

    let error =
        ironflow::scheduler::startup::validate_schedule_flows(&schedules, Some(flows_dir.path()))
            .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("nightly") && message.contains("reports/nightl.lua"),
        "error should name the schedule and the unresolved flow path: {message}"
    );
}

#[test]
fn a_schedule_naming_a_resolvable_flow_passes_startup_validation() {
    let flows = flows_with_logger();
    let schedules =
        std::collections::HashMap::from([("nightly".to_string(), nightly("nightly.lua"))]);

    assert!(
        ironflow::scheduler::startup::validate_schedule_flows(&schedules, Some(flows.path()))
            .is_ok()
    );
}
