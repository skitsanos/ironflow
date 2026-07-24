pub mod error;
pub mod event_store;
pub mod json_store;
pub mod lifecycle;
pub mod null_store;
#[cfg(feature = "redis")]
mod redis_config;
#[cfg(feature = "redis")]
mod redis_keys;
#[cfg(feature = "redis")]
pub mod redis_store;
pub mod run_id;
mod run_listing;
pub mod sql_names;
pub mod sql_store;

pub use error::{StorageError, StorageErrorKind, StorageResult};
#[cfg(feature = "redis")]
pub use event_store::RedisEventStore;
pub use event_store::{EventStore, MemoryEventStore, SqlEventStore};
pub use run_id::{MAX_RUN_ID_BYTES, RunIdError, validate_run_id};
pub use run_listing::{PageSize, RunCursor, RunListQuery, RunSummaryPage};
pub use sql_store::SqlStateStore;

use async_trait::async_trait;

use crate::engine::types::*;

/// Trait for workflow state persistence.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Initialize a new workflow run.
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()>;

    /// Update the overall run status. Terminal statuses must stamp `finished`;
    /// the state record is authoritative when best-effort event publication
    /// fails.
    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()>;

    /// Create or update a task's state within a run.
    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()>;

    /// Get the current context for a run.
    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context>;

    /// Merge updates into the run's context.
    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()>;

    /// Get full run information.
    async fn get_run_info(&self, run_id: &str) -> StorageResult<RunInfo>;

    /// List runs, optionally filtered by status.
    async fn list_runs(&self, status: Option<RunStatus>) -> StorageResult<Vec<RunInfo>>;

    /// List run summaries — cheaper than `list_runs` because it can skip
    /// loading full `ctx` and per-task history. Default implementation falls
    /// back to `list_runs`; concrete stores SHOULD override with a primitive
    /// that reads only the summary fields.
    async fn list_run_summaries(
        &self,
        status: Option<RunStatus>,
    ) -> StorageResult<Vec<RunSummary>> {
        let runs = self.list_runs(status).await?;
        Ok(runs.iter().map(RunSummary::from).collect())
    }

    /// Return one bounded, deterministic page of lightweight run summaries.
    /// User-facing list operations must use this primitive rather than either
    /// unbounded compatibility method above.
    async fn list_run_summaries_page(&self, query: &RunListQuery) -> StorageResult<RunSummaryPage>;

    /// Delete a run record.
    async fn delete_run(&self, run_id: &str) -> StorageResult<()>;

    /// Delete runs older than the given cutoff (UTC). Returns the number
    /// removed. Default implementation scans via `list_runs`; stores that
    /// track metadata separately MAY override with an index-only path.
    async fn prune_before(&self, cutoff: chrono::DateTime<chrono::Utc>) -> StorageResult<usize> {
        let runs = self.list_runs(None).await?;
        let mut removed = 0;
        for r in runs {
            if r.started.map(|t| t < cutoff).unwrap_or(false) && r.status.is_terminal() {
                match self.delete_run(&r.id).await {
                    Ok(()) => removed += 1,
                    Err(error) if error.is_not_found() => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(removed)
    }
}
