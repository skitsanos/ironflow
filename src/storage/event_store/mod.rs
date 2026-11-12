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

/// Longest accepted incarnation token. The engine uses a 32-character run
/// lease owner; the bound keeps a caller-chosen token out of unbounded keys.
pub const MAX_EVENT_INCARNATION_BYTES: usize = 128;

fn validate_incarnation(incarnation: &str) -> StorageResult<()> {
    if incarnation.is_empty() || incarnation.len() > MAX_EVENT_INCARNATION_BYTES {
        return Err(StorageError::invalid_input(format_args!(
            "Event incarnation must contain 1 through {MAX_EVENT_INCARNATION_BYTES} bytes"
        )));
    }
    Ok(())
}

fn deleted_stream(run_id: &str) -> StorageError {
    StorageError::conflict(format_args!("Event stream for run '{run_id}' was deleted"))
}

#[async_trait]
pub trait EventStore: Send + Sync {
    /// Lightweight backend availability probe used by service readiness.
    async fn healthcheck(&self) -> StorageResult<()> {
        Ok(())
    }

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
    ///
    /// A publication that names no incarnation is rejected for as long as the
    /// backend retains a deletion fence for the run, including after
    /// [`begin_incarnation`](Self::begin_incarnation) reopened the stream.
    async fn publish(&self, event: RunEvent) -> StorageResult<()>;

    /// Publish one event on behalf of one run incarnation.
    ///
    /// Identical to [`publish`](Self::publish) for a run that was never
    /// deleted. While a deletion fence is retained, only the incarnation that
    /// [`begin_incarnation`](Self::begin_incarnation) last admitted may
    /// publish; every other incarnation, including the deleted one, is a
    /// conflict. The default suits stores without a fence.
    async fn publish_as(&self, event: RunEvent, _incarnation: &str) -> StorageResult<()> {
        self.publish(event).await
    }

    /// Admit `incarnation` as the sole publisher past a retained deletion
    /// fence for `run_id`.
    ///
    /// Call it once the state store has accepted the run ID again. It records
    /// nothing for a run that has no fence, and a later
    /// [`delete_run`](Self::delete_run) closes the stream to this incarnation
    /// too.
    async fn begin_incarnation(&self, _run_id: &str, _incarnation: &str) -> StorageResult<()> {
        Ok(())
    }

    /// Delete every retained event for `run_id` and fence later publication.
    ///
    /// Event cleanup is idempotent: an absent stream is not an error. A later
    /// publication for the deleted run must fail while the backend retains
    /// the fence, until [`begin_incarnation`](Self::begin_incarnation) admits
    /// a new incarnation. The returned count is the number of event payloads
    /// removed.
    async fn delete_run(&self, run_id: &str) -> StorageResult<usize>;

    /// Remove events whose run state is already absent.
    ///
    /// Unlike [`delete_run`](Self::delete_run), this installs a fence only
    /// when it removed at least one event, so deleting an ID that never
    /// existed cannot mute a later run. An existing fence is left as it is.
    /// The returned count lets a lifecycle coordinator distinguish an absent
    /// run from orphaned events left by an interrupted earlier deletion.
    async fn delete_orphaned_run(&self, run_id: &str) -> StorageResult<usize> {
        self.delete_run(run_id).await
    }

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
