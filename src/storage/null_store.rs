use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::engine::types::*;
use crate::storage::{RunListQuery, RunSummaryPage, StateStore, StorageError, StorageResult};

/// In-memory state store for subworkflow execution.
/// Holds run state only for the lifetime of the store instance.
pub struct NullStateStore {
    runs: Mutex<HashMap<String, RunInfo>>,
}

impl NullStateStore {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            runs: Mutex::new(HashMap::new()),
        }
    }

    fn lock_runs(&self) -> StorageResult<std::sync::MutexGuard<'_, HashMap<String, RunInfo>>> {
        self.runs
            .lock()
            .map_err(|error| StorageError::backend("Failed to lock in-memory state store", error))
    }
}

// Allow construction via Default trait pattern
impl Default for NullStateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StateStore for NullStateStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()> {
        let run_info = RunInfo {
            id: run_id.to_string(),
            flow_name: flow_name.to_string(),
            status: RunStatus::Pending,
            started: Some(chrono::Utc::now()),
            finished: None,
            ctx: ctx.clone(),
            tasks: HashMap::new(),
        };
        let mut runs = self.lock_runs()?;
        if runs.contains_key(run_id) {
            return Err(StorageError::conflict(format_args!(
                "Run '{run_id}' already exists"
            )));
        }
        runs.insert(run_id.to_string(), run_info);
        Ok(())
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        let mut runs = self.lock_runs()?;
        let run = runs
            .get_mut(run_id)
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))?;
        let is_terminal = status.is_terminal();
        run.status = status;
        if is_terminal && run.finished.is_none() {
            run.finished = Some(chrono::Utc::now());
        }
        Ok(())
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()> {
        let mut runs = self.lock_runs()?;
        let run = runs
            .get_mut(run_id)
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))?;
        run.tasks.insert(task.name.clone(), task.clone());
        Ok(())
    }

    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context> {
        let runs = self.lock_runs()?;
        runs.get(run_id)
            .map(|r| r.ctx.clone())
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()> {
        let mut runs = self.lock_runs()?;
        let run = runs
            .get_mut(run_id)
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))?;
        for (k, v) in ctx {
            run.ctx.insert(k.clone(), v.clone());
        }
        Ok(())
    }

    async fn get_run_info(&self, run_id: &str) -> StorageResult<RunInfo> {
        let runs = self.lock_runs()?;
        runs.get(run_id)
            .cloned()
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))
    }

    async fn list_runs(&self, _status: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        Ok(Vec::new())
    }

    async fn list_run_summaries_page(
        &self,
        _query: &RunListQuery,
    ) -> StorageResult<RunSummaryPage> {
        Ok(RunSummaryPage::empty())
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<()> {
        self.lock_runs()?
            .remove(run_id)
            .map(|_| ())
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))
    }
}
