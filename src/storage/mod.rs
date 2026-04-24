pub mod json_store;
pub mod null_store;
#[cfg(feature = "redis")]
pub mod redis_store;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::*;

/// Trait for workflow state persistence.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Initialize a new workflow run.
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> Result<()>;

    /// Update the overall run status.
    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> Result<()>;

    /// Create or update a task's state within a run.
    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> Result<()>;

    /// Get the current context for a run.
    async fn get_ctx(&self, run_id: &str) -> Result<Context>;

    /// Merge updates into the run's context.
    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> Result<()>;

    /// Get full run information.
    async fn get_run_info(&self, run_id: &str) -> Result<RunInfo>;

    /// List runs, optionally filtered by status.
    async fn list_runs(&self, status: Option<RunStatus>) -> Result<Vec<RunInfo>>;

    /// List run summaries — cheaper than `list_runs` because it can skip
    /// loading full `ctx` and per-task history. Default implementation falls
    /// back to `list_runs`; concrete stores SHOULD override with a primitive
    /// that reads only the summary fields.
    async fn list_run_summaries(&self, status: Option<RunStatus>) -> Result<Vec<RunSummary>> {
        let runs = self.list_runs(status).await?;
        Ok(runs.iter().map(RunSummary::from).collect())
    }

    /// Delete a run record.
    async fn delete_run(&self, run_id: &str) -> Result<()>;

    /// Delete runs older than the given cutoff (UTC). Returns the number
    /// removed. Default implementation scans via `list_runs`; stores that
    /// track metadata separately MAY override with an index-only path.
    async fn prune_before(&self, cutoff: chrono::DateTime<chrono::Utc>) -> Result<usize> {
        let runs = self.list_runs(None).await?;
        let mut removed = 0;
        for r in runs {
            if r.started.map(|t| t < cutoff).unwrap_or(false)
                && r.status.is_terminal()
                && self.delete_run(&r.id).await.is_ok()
            {
                removed += 1;
            }
        }
        Ok(removed)
    }
}
