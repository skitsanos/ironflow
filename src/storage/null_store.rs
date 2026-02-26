use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::*;
use crate::storage::StateStore;

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
}

// Allow construction via Default trait pattern
impl Default for NullStateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StateStore for NullStateStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> Result<()> {
        let run_info = RunInfo {
            id: run_id.to_string(),
            flow_name: flow_name.to_string(),
            status: RunStatus::Pending,
            started: Some(chrono::Utc::now()),
            finished: None,
            ctx: ctx.clone(),
            tasks: HashMap::new(),
        };
        self.runs
            .lock()
            .unwrap()
            .insert(run_id.to_string(), run_info);
        Ok(())
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> Result<()> {
        if let Some(run) = self.runs.lock().unwrap().get_mut(run_id) {
            run.status = status;
            if run.finished.is_none() {
                run.finished = Some(chrono::Utc::now());
            }
        }
        Ok(())
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> Result<()> {
        if let Some(run) = self.runs.lock().unwrap().get_mut(run_id) {
            run.tasks.insert(task.name.clone(), task.clone());
        }
        Ok(())
    }

    async fn get_ctx(&self, run_id: &str) -> Result<Context> {
        let runs = self.runs.lock().unwrap();
        runs.get(run_id)
            .map(|r| r.ctx.clone())
            .ok_or_else(|| anyhow::anyhow!("Run not found: {}", run_id))
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> Result<()> {
        if let Some(run) = self.runs.lock().unwrap().get_mut(run_id) {
            for (k, v) in ctx {
                run.ctx.insert(k.clone(), v.clone());
            }
        }
        Ok(())
    }

    async fn get_run_info(&self, run_id: &str) -> Result<RunInfo> {
        let runs = self.runs.lock().unwrap();
        runs.get(run_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Run not found: {}", run_id))
    }

    async fn list_runs(&self, _status: Option<RunStatus>) -> Result<Vec<RunInfo>> {
        Ok(Vec::new())
    }

    async fn delete_run(&self, run_id: &str) -> Result<()> {
        self.runs.lock().unwrap().remove(run_id);
        Ok(())
    }
}
