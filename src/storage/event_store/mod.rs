use async_trait::async_trait;

use crate::engine::events::RunEvent;
use crate::storage::{MAX_RUN_ID_BYTES, StorageError, StorageResult};

pub mod memory;
#[cfg(feature = "redis")]
pub mod redis;
pub mod sql;

pub use memory::{
    DEFAULT_MEMORY_EVENT_BYTE_CAPACITY, DEFAULT_MEMORY_EVENT_CAPACITY, MemoryEventStore,
};
#[cfg(feature = "redis")]
pub use redis::RedisEventStore;
pub use sql::SqlEventStore;

fn validate_publish_event(event: &RunEvent) -> StorageResult<()> {
    validate_event_run_id(&event.run_id)?;
    if event.id.is_empty() {
        return Err(StorageError::invalid_input(
            "Event ID must be non-empty because it is the replay cursor",
        ));
    }
    Ok(())
}

fn validate_event_run_id(run_id: &str) -> StorageResult<()> {
    if run_id.is_empty() || run_id.len() > MAX_RUN_ID_BYTES {
        return Err(StorageError::invalid_input(format_args!(
            "Event run ID must contain 1 through {MAX_RUN_ID_BYTES} bytes"
        )));
    }
    Ok(())
}

#[async_trait]
pub trait EventStore: Send + Sync {
    /// Publish one logical event with a non-empty ID unique within its run.
    ///
    /// Callers must never assign that ID to another logical event in the same
    /// run, even after backend retention or TTL expiry may discard the original
    /// identity. Retrying the same `(run_id, id, payload)` is idempotent while
    /// that identity remains retained. Reusing a retained ID with a different
    /// payload is a conflict, but bounded and TTL-backed stores can detect that
    /// conflict only while they retain the prior identity. Another run may use
    /// the same opaque ID independently. Engine-generated UUIDv4 event IDs
    /// satisfy this caller obligation.
    async fn publish(&self, event: RunEvent) -> StorageResult<()>;

    /// Delete every retained event for `run_id` and fence later publication.
    ///
    /// Event cleanup is idempotent: an absent stream is not an error. A later
    /// [`publish`](Self::publish) for the deleted run must fail while
    /// the backend retains the fence. The returned count is the number of
    /// event payloads removed, which lets a lifecycle coordinator distinguish
    /// an already-absent run from orphaned events left by an interrupted
    /// earlier deletion.
    async fn delete_run(&self, run_id: &str) -> StorageResult<usize>;

    /// Lists at most `limit` events after an opaque event-ID cursor.
    ///
    /// `None` and an empty cursor start at the beginning of the run's event
    /// stream. A non-empty cursor must identify an event in that same stream;
    /// unknown, expired, and cross-run cursors return `StorageErrorKind::NotFound`.
    /// The cursor itself is excluded. Each backend returns subsequent pages in
    /// its stable read order, so concatenating pages by their last returned ID
    /// neither repeats nor skips retained events. A zero limit still validates
    /// a supplied cursor before returning an empty batch.
    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> StorageResult<Vec<RunEvent>>;
}
