#![cfg(feature = "redis")]

#[path = "redis_atomicity/event.rs"]
mod event;
#[path = "redis_atomicity/event_delete.rs"]
mod event_delete;
#[path = "redis_atomicity/event_faults.rs"]
mod event_faults;
#[path = "redis_atomicity/event_legacy_bounded.rs"]
mod event_legacy_bounded;
#[path = "redis_atomicity/event_legacy_expiry.rs"]
mod event_legacy_expiry;
#[path = "redis_atomicity/event_legacy_limits.rs"]
mod event_legacy_limits;
#[path = "redis_atomicity/event_legacy_race.rs"]
mod event_legacy_race;
#[path = "redis_atomicity/event_legacy_recovery.rs"]
mod event_legacy_recovery;
#[path = "redis_atomicity/event_migration_collision.rs"]
mod event_migration_collision;
#[path = "redis_atomicity/migration.rs"]
mod migration;
#[path = "support/redis.rs"]
mod redis_support;
#[path = "redis_atomicity/state.rs"]
mod state;
#[path = "redis_atomicity/state_catalog_recovery.rs"]
mod state_catalog_recovery;
#[path = "redis_atomicity/state_faults.rs"]
mod state_faults;
#[path = "redis_atomicity/state_maintenance.rs"]
mod state_maintenance;
#[path = "redis_atomicity/state_sweep.rs"]
mod state_sweep;
#[path = "redis_atomicity/ttl.rs"]
mod ttl;

use ironflow::engine::types::{TaskState, TaskStatus};
use tokio::task::JoinSet;

const WRITERS: usize = 24;

fn task(index: usize) -> TaskState {
    TaskState {
        name: format!("task-{index}"),
        node_type: "log".to_string(),
        status: TaskStatus::Success,
        attempt: 1,
        started: Some(chrono::Utc::now()),
        finished: Some(chrono::Utc::now()),
        input: None,
        output: Some(serde_json::json!({"writer": index})),
        error: None,
    }
}

async fn finish_writers<E>(mut writers: JoinSet<Result<(), E>>)
where
    E: std::fmt::Debug + 'static,
{
    while let Some(result) = writers.join_next().await {
        result.expect("Redis writer task panicked").unwrap();
    }
}
