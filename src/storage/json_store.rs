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

    /// Sidecar file holding only the `RunSummary` for this run.
    ///
    /// Written alongside every full-record update so listings can enumerate
    /// tiny files instead of re-parsing multi-MB run blobs. The two files are
    /// kept consistent by always writing the summary after the main file —
    /// a stale summary is just missed until the next write, whereas a
    /// dangling summary with no run is pruned by `list_run_summaries`.
    fn summary_path(&self, run_id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.summary.json", run_id))
    }

    async fn read_run(&self, run_id: &str) -> Result<RunInfo> {
        let path = self.run_path(run_id);
        let data = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read run file: {}", path.display()))?;
        let info: RunInfo = serde_json::from_str(&data)
            .with_context(|| format!("Failed to parse run: {}", run_id))?;
        Ok(info)
    }

    async fn write_run(&self, run_id: &str, info: &RunInfo) -> Result<()> {
        let path = self.run_path(run_id);
        let tmp_path = path.with_extension("json.tmp");

        let data = serde_json::to_string_pretty(info)?;
        tokio::fs::write(&tmp_path, &data).await?;
        tokio::fs::rename(&tmp_path, &path).await?;

        // Sidecar summary. Failure here is non-fatal — the main record is the
        // source of truth; a missing summary just makes the next listing do
        // a full parse for this run.
        let summary = RunSummary::from(info);
        let summary_path = self.summary_path(run_id);
        let summary_tmp = summary_path.with_extension("json.tmp");
        if let Ok(sjson) = serde_json::to_string(&summary) {
            let _ = tokio::fs::write(&summary_tmp, &sjson).await;
            let _ = tokio::fs::rename(&summary_tmp, &summary_path).await;
        }

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
        let is_terminal = status.is_terminal();
        info.status = status;
        if is_terminal {
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
                && let Ok(info) = serde_json::from_str::<RunInfo>(&data)
            {
                if let Some(ref filter) = status_filter
                    && &info.status != filter
                {
                    continue;
                }
                runs.push(info);
            }
        }

        // Sort by start time, newest first
        runs.sort_by_key(|run| std::cmp::Reverse(run.started));

        Ok(runs)
    }

    async fn delete_run(&self, run_id: &str) -> Result<()> {
        let _lock = self.lock.write().await;
        let path = self.run_path(run_id);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        let summary = self.summary_path(run_id);
        if summary.exists() {
            let _ = tokio::fs::remove_file(&summary).await;
        }
        Ok(())
    }

    /// Native summary listing — reads only `*.summary.json` sidecar files.
    /// Falls back to parsing the main record for any run missing a sidecar
    /// (covers data written before the sidecar was introduced).
    async fn list_run_summaries(
        &self,
        status_filter: Option<RunStatus>,
    ) -> Result<Vec<RunSummary>> {
        let _lock = self.lock.read().await;

        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }

        let mut summaries = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            // Only consider main records; summaries are an optimization, not
            // authoritative. For each main record we try the summary first
            // and fall back to a full parse.
            if !name.ends_with(".json") || name.ends_with(".summary.json") {
                continue;
            }
            let run_id = &name[..name.len() - ".json".len()];

            let summary_path = self.summary_path(run_id);
            let summary: Option<RunSummary> = if summary_path.exists() {
                tokio::fs::read_to_string(&summary_path)
                    .await
                    .ok()
                    .and_then(|data| serde_json::from_str::<RunSummary>(&data).ok())
            } else {
                None
            };

            let summary = match summary {
                Some(s) => s,
                None => {
                    // Sidecar missing or corrupt — fall back to a full parse.
                    let data = tokio::fs::read_to_string(&path).await?;
                    match serde_json::from_str::<RunInfo>(&data) {
                        Ok(info) => RunSummary::from(&info),
                        Err(_) => continue,
                    }
                }
            };

            if let Some(ref filter) = status_filter
                && &summary.status != filter
            {
                continue;
            }
            summaries.push(summary);
        }

        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.started));
        Ok(summaries)
    }
}
