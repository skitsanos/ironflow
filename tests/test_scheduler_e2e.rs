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
