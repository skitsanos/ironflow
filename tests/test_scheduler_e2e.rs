use std::sync::Arc;

use chrono::{TimeZone as _, Utc};
use ironflow::engine::types::Context;
use ironflow::scheduler::config::ScheduleConfig;
use ironflow::scheduler::execution::SCHEDULE_CONTEXT_KEY;
use ironflow::scheduler::{Outcome, ScheduleExecutor, Scheduler};
use ironflow::storage::StateStore;

#[path = "support/scheduler.rs"]
mod scheduler_support;

use scheduler_support::{build_executor, flows_with_logger};

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
async fn overlap_detection_finds_a_non_terminal_run_of_the_same_schedule() {
    let flows = flows_with_logger();
    let app = build_executor(flows.path());

    // No run yet.
    assert!(app.executor.active_run("nightly").await.is_none());

    // A completed run is not an overlap.
    app.executor
        .run("nightly", &nightly("nightly.lua"))
        .await
        .unwrap();
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

    // Every minute, so the first tick has something due within the grace
    // window without waiting for a wall-clock hour.
    let mut ctx = Context::new();
    ctx.insert("region".to_string(), serde_json::json!("eu"));
    let schedule = ScheduleConfig::new("nightly.lua", "* * * * *", Some("UTC"), None, ctx).unwrap();

    let handle = ironflow::scheduler::spawn(
        std::collections::HashMap::from([("frequent".to_string(), schedule)]),
        store.clone() as Arc<dyn StateStore>,
        events,
        Some(flows.path().to_path_buf()),
        None,
    )
    .expect("a non-empty schedule map must spawn a scheduler");

    // The loop evaluates immediately on entry, then on each tick. Catch-up is
    // seeded at now - grace, so the previous minute boundary is due.
    let mut runs = Vec::new();
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        runs = store.list_runs(None).await.unwrap();
        if !runs.is_empty() {
            break;
        }
    }
    handle.abort();

    assert_eq!(runs.len(), 1, "the tick loop did not fire a due schedule");
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
