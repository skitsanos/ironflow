pub mod json_store;

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

    /// Delete a run record.
    async fn delete_run(&self, run_id: &str) -> Result<()>;
}
