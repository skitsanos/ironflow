use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use crate::engine::types::*;
use crate::storage::StateStore;

/// File-based JSON state store. Each run is stored as a separate JSON file.
pub struct JsonStateStore {
    base_dir: PathBuf,
    lock: RwLock<()>,
}

impl JsonStateStore {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
            lock: RwLock::new(()),
        }
    }

    fn run_path(&self, run_id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", run_id))
    }

    async fn read_run(&self, run_id: &str) -> Result<RunInfo> {
        let path = self.run_path(run_id);
        let data = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read run file: {}", path.display()))?;
        let info: RunInfo =
            serde_json::from_str(&data).with_context(|| format!("Failed to parse run: {}", run_id))?;
        Ok(info)
    }

    async fn write_run(&self, run_id: &str, info: &RunInfo) -> Result<()> {
        let path = self.run_path(run_id);
        let tmp_path = path.with_extension("json.tmp");

        let data = serde_json::to_string_pretty(info)?;
        tokio::fs::write(&tmp_path, &data).await?;
        tokio::fs::rename(&tmp_path, &path).await?;

        Ok(())
    }
}

#[async_trait]
impl StateStore for JsonStateStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> Result<()> {
        let _lock = self.lock.write().await;

        // Ensure the directory exists
        tokio::fs::create_dir_all(&self.base_dir).await?;

        let info = RunInfo {
            id: run_id.to_string(),
            flow_name: flow_name.to_string(),
            status: RunStatus::Pending,
            started: Some(Utc::now()),
            finished: None,
            ctx: ctx.clone(),
            tasks: HashMap::new(),
        };

        self.write_run(run_id, &info).await
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> Result<()> {
        let _lock = self.lock.write().await;
        let mut info = self.read_run(run_id).await?;
        info.status = status.clone();
        if matches!(status, RunStatus::Success | RunStatus::Failed | RunStatus::Stalled) {
            info.finished = Some(Utc::now());
        }
        self.write_run(run_id, &info).await
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> Result<()> {
        let _lock = self.lock.write().await;
        let mut info = self.read_run(run_id).await?;
        info.tasks.insert(task.name.clone(), task.clone());
        self.write_run(run_id, &info).await
    }

    async fn get_ctx(&self, run_id: &str) -> Result<Context> {
        let _lock = self.lock.read().await;
        let info = self.read_run(run_id).await?;
        Ok(info.ctx)
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> Result<()> {
        let _lock = self.lock.write().await;
        let mut info = self.read_run(run_id).await?;
        for (k, v) in ctx {
            info.ctx.insert(k.clone(), v.clone());
        }
        self.write_run(run_id, &info).await
    }

    async fn get_run_info(&self, run_id: &str) -> Result<RunInfo> {
        let _lock = self.lock.read().await;
        self.read_run(run_id).await
    }

    async fn list_runs(&self, status_filter: Option<RunStatus>) -> Result<Vec<RunInfo>> {
        let _lock = self.lock.read().await;

        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }

        let mut runs = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                && let Ok(data) = tokio::fs::read_to_string(&path).await
                    && let Ok(info) = serde_json::from_str::<RunInfo>(&data) {
                        if let Some(ref filter) = status_filter
                            && &info.status != filter {
                                continue;
                            }
                        runs.push(info);
                    }
        }

        // Sort by start time, newest first
        runs.sort_by(|a, b| b.started.cmp(&a.started));

        Ok(runs)
    }

    async fn delete_run(&self, run_id: &str) -> Result<()> {
        let _lock = self.lock.write().await;
        let path = self.run_path(run_id);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }
}
