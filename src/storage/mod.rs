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
pub mod run_lease;
mod run_listing;
mod run_reaper;
pub(crate) mod sql_ddl;
pub mod sql_names;
pub mod sql_store;

pub use error::{StorageError, StorageErrorKind, StorageResult};
#[cfg(feature = "redis")]
pub use event_store::RedisEventStore;
pub use event_store::{EventStore, MemoryEventStore, SqlEventStore};
pub use run_id::{MAX_RUN_ID_BYTES, RunIdError, validate_run_id};
pub use run_lease::{RUN_LEASE_REFRESH, RUN_LEASE_TTL, RunLease};
pub use run_listing::{PageSize, RunCursor, RunListQuery, RunSummaryPage};
pub(crate) use run_reaper::{RunLeaseReaper, spawn_run_lease_reaper};
pub use sql_store::SqlStateStore;

use async_trait::async_trait;

use crate::engine::types::*;

/// Trait for workflow state persistence.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Initialize a new workflow run.
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()>;

    /// Initialize a run together with its execution-owner lease.
    ///
    /// The compatibility default keeps third-party/in-memory stores working;
    /// durable built-in stores override this with an atomic commit. Stores
    /// using the default also use the permissive owned-status defaults below
    /// and opt out of startup reconciliation rather than risking a false
    /// `Stalled` transition.
    async fn init_run_owned(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
        _lease: &RunLease,
    ) -> StorageResult<()> {
        self.init_run(run_id, flow_name, ctx).await
    }

    /// Update the overall run status. Terminal statuses must stamp `finished`;
    /// the state record is authoritative when best-effort event publication
    /// fails.
    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()>;

    /// Update status only while `owner` still owns the run. Returns `false`
    /// when ownership was lost. A terminal transition also releases the lease.
    async fn set_run_status_owned(
        &self,
        run_id: &str,
        status: RunStatus,
        _owner: &str,
    ) -> StorageResult<bool> {
        self.set_run_status(run_id, status).await?;
        Ok(true)
    }

    /// Extend an active run's lease. Returns `false` when the run is terminal
    /// or ownership no longer matches.
    async fn renew_run_lease(&self, _run_id: &str, _lease: &RunLease) -> StorageResult<bool> {
        Ok(true)
    }

    /// Atomically stall only non-terminal runs whose ownership lease expired.
    /// Unleased legacy runs are deliberately ignored for rolling-upgrade
    /// safety: a new replica cannot know whether an old replica is still live.
    async fn reconcile_expired_run_leases(
        &self,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> StorageResult<usize> {
        Ok(0)
    }

    /// Create or update a task's state within a run.
    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()>;

    /// Persist task state only while `owner` holds an unexpired run lease.
    async fn upsert_task_owned(
        &self,
        run_id: &str,
        task: &TaskState,
        _owner: &str,
    ) -> StorageResult<bool> {
        self.upsert_task(run_id, task).await?;
        Ok(true)
    }

    /// Get the current context for a run.
    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context>;

    /// Merge updates into the run's context.
    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()>;

    /// Merge context only while `owner` holds an unexpired run lease.
    async fn update_ctx_owned(
        &self,
        run_id: &str,
        ctx: &Context,
        _owner: &str,
    ) -> StorageResult<bool> {
        self.update_ctx(run_id, ctx).await?;
        Ok(true)
    }

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

    /// Delete a run record atomically. Built-in durable stores return
    /// [`StorageErrorKind::Conflict`] while a non-terminal run still has a
    /// live execution-owner lease; terminal and abandoned runs remain
    /// deletable.
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

    /// Claim one scheduled instant, returning `true` for the single caller
    /// across every process sharing this store that owns it.
    ///
    /// `name` is the schedule's configured name and `key` its local wall-clock
    /// identity. `ttl_seconds` bounds how long the record is retained; it must
    /// exceed the schedule's grace window, or a late replica could re-fire an
    /// instant whose claim has already been reaped.
    ///
    /// The default refuses. A store that cannot coordinate claims must not
    /// quietly let every replica fire — that is the duplicate this method
    /// exists to prevent — so scheduling is unavailable rather than unsafe.
    async fn claim_schedule(&self, name: &str, key: &str, ttl_seconds: u64) -> StorageResult<bool> {
        let _ = (name, key, ttl_seconds);
        Err(StorageError::backend(
            "Claim scheduled instant",
            "this state store does not support scheduling",
        ))
    }
}

/// Prune terminal runs older than `cutoff` by walking bounded newest-first
/// summary pages instead of loading the entire catalog into memory. Uses
/// O(page-size) memory. Delete-safe: the keyset cursor is anchored on
/// `(started, id)`, so removing an already-visited run does not shift the
/// position of later pages (IF-051).
pub(crate) async fn prune_before_via_summary_pages(
    store: &(impl StateStore + ?Sized),
    cutoff: chrono::DateTime<chrono::Utc>,
) -> StorageResult<usize> {
    let page_size = PageSize::new(256)?;
    let mut removed = 0;
    let mut after: Option<RunCursor> = None;
    loop {
        let query = RunListQuery::new(None, after, page_size)?;
        let page = store.list_run_summaries_page(&query).await?;
        for summary in &page.items {
            if summary.status.is_terminal()
                && summary.started.is_some_and(|started| started < cutoff)
            {
                match store.delete_run(&summary.id).await {
                    Ok(()) => removed += 1,
                    Err(error) if error.is_not_found() => {}
                    Err(error) => return Err(error),
                }
            }
        }
        match page.next {
            Some(cursor) => after = Some(cursor),
            None => return Ok(removed),
        }
    }
}

/// Reconcile runs whose execution-owner leases expired after a crash/kill.
/// Store implementations fence heartbeat, terminalization, and reconciliation
/// against the same lease record so a newly starting replica cannot stall a
/// peer's live run.
pub async fn reconcile_nonterminal_runs(
    store: &(impl StateStore + ?Sized),
) -> StorageResult<usize> {
    store.reconcile_expired_run_leases(chrono::Utc::now()).await
}
