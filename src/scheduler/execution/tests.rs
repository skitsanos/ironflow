use std::collections::HashMap;

use chrono::{TimeZone as _, Utc};

use super::*;
use crate::scheduler::{Outcome, Scheduler};
use crate::storage::event_store::MemoryEventStore;
use crate::storage::json_store::JsonStateStore;

const KEY: &str = "UTC@2026-05-01T02:00";

/// IF-217 (g): a skip is now durable, so a replica that is only draining must
/// not report one. It defers instead: nothing is recorded, and a peer (or
/// this replica after its restart) still starts the instant.
#[tokio::test]
async fn a_draining_replica_defers_the_instant_and_records_nothing() {
    let flows = tempfile::tempdir().unwrap();
    std::fs::write(
        flows.path().join("job.lua"),
        "local flow = Flow.new('job')\nflow:step('emit', nodes.log({ message = 'ran' }))\nreturn flow",
    )
    .unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(store_dir.path()));
    let lifecycle = crate::api::ServiceLifecycle::default();
    let executor = Arc::new(FlowExecutor::new_with_lifecycle(
        Arc::new(NodeRegistry::with_builtins()),
        store.clone(),
        Arc::new(MemoryEventStore::new()),
        Some(flows.path().to_path_buf()),
        None,
        lifecycle.clone(),
        None,
    ));
    let schedule = ScheduleConfig::new(
        "job.lua",
        "0 2 * * *",
        Some("UTC"),
        None,
        crate::engine::types::Context::new(),
    )
    .unwrap();
    let now = Utc.with_ymd_and_hms(2026, 5, 1, 2, 0, 5).unwrap();
    let mut scheduler = Scheduler::new(
        HashMap::from([("nightly".to_string(), schedule)]),
        store.clone(),
        executor,
        now,
    );

    lifecycle.begin_draining();
    let decisions = scheduler.evaluate(now).await;

    assert_eq!(decisions.len(), 1, "{decisions:?}");
    assert!(
        matches!(&decisions[0].outcome, Outcome::Deferred { reason } if reason.contains("draining")),
        "{decisions:?}"
    );
    assert!(!store.schedule_run_recorded("nightly", KEY).await.unwrap());
    assert!(store.list_runs(None).await.unwrap().is_empty());
}
