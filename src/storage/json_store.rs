use std::collections::HashMap;
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize};
#[cfg(test)]
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use crate::engine::types::*;
use crate::storage::run_listing::compare_summaries;
use crate::storage::{RunListQuery, RunSummaryPage, StateStore, StorageError, StorageResult};

#[cfg(test)]
mod cancellation_tests;
mod catalog;
mod codec;
mod fs;
mod listing;
mod platform;
mod records;
#[cfg(test)]
mod revision_tests;
mod temp;
#[cfg(test)]
mod test_support;

use fs::{FileState, SecureStoreDir};

/// File-based JSON state store. Each run is stored as a separate JSON file.
pub struct JsonStateStore {
    directory: SecureStoreDir,
    lock: RwLock<()>,
    #[cfg(test)]
    fail_next_summary_commit: AtomicBool,
    #[cfg(test)]
    directory_entries_examined: AtomicUsize,
    #[cfg(test)]
    current_summary_reads: AtomicUsize,
    #[cfg(test)]
    catalog_read_hook: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    #[cfg(test)]
    catalog_rebuild_hook: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
}

impl JsonStateStore {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            directory: SecureStoreDir::new(base_dir.as_ref().to_path_buf()),
            lock: RwLock::new(()),
            #[cfg(test)]
            fail_next_summary_commit: AtomicBool::new(false),
            #[cfg(test)]
            directory_entries_examined: AtomicUsize::new(0),
            #[cfg(test)]
            current_summary_reads: AtomicUsize::new(0),
            #[cfg(test)]
            catalog_read_hook: Mutex::new(None),
            #[cfg(test)]
            catalog_rebuild_hook: Mutex::new(None),
        }
    }
}

#[async_trait]
impl StateStore for JsonStateStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()> {
        codec::validate_input_id(run_id)?;
        let _lock = self.lock.write().await;
        self.directory.ensure_created().await?;
        self.directory
            .inspect_regular(&Self::summary_name(run_id))
            .await?;
        if self
            .directory
            .inspect_regular(&Self::run_name(run_id))
            .await?
            == FileState::Regular
        {
            return Err(StorageError::conflict(format_args!(
                "Run '{run_id}' already exists"
            )));
        }
        let mut catalog = catalog::CatalogTransaction::begin(self).await?;
        let info = RunInfo {
            id: run_id.to_string(),
            flow_name: flow_name.to_string(),
            status: RunStatus::Pending,
            started: Some(Utc::now()),
            finished: None,
            ctx: ctx.clone(),
            tasks: HashMap::new(),
        };
        let record = self.write_new_run(&mut catalog, run_id, &info).await?;
        self.upsert_catalog_best_effort(run_id, catalog, record)
            .await;
        Ok(())
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        codec::validate_input_id(run_id)?;
        let _lock = self.lock.write().await;
        let mut catalog = catalog::CatalogTransaction::begin(self).await?;
        let mut info = self.read_run(run_id).await?;
        let is_terminal = status.is_terminal();
        info.status = status;
        if is_terminal {
            info.finished = Some(Utc::now());
        }
        let record = self.write_existing_run(&mut catalog, run_id, &info).await?;
        self.upsert_catalog_best_effort(run_id, catalog, record)
            .await;
        Ok(())
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()> {
        codec::validate_input_id(run_id)?;
        let _lock = self.lock.write().await;
        let mut catalog = catalog::CatalogTransaction::begin(self).await?;
        let mut info = self.read_run(run_id).await?;
        info.tasks.insert(task.name.clone(), task.clone());
        self.write_existing_run(&mut catalog, run_id, &info).await?;
        self.commit_catalog_unchanged_best_effort(run_id, catalog)
            .await;
        Ok(())
    }

    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context> {
        codec::validate_input_id(run_id)?;
        let _lock = self.lock.read().await;
        Ok(self.read_run(run_id).await?.ctx)
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()> {
        codec::validate_input_id(run_id)?;
        let _lock = self.lock.write().await;
        let mut catalog = catalog::CatalogTransaction::begin(self).await?;
        let mut info = self.read_run(run_id).await?;
        info.ctx.extend(ctx.clone());
        self.write_existing_run(&mut catalog, run_id, &info).await?;
        self.commit_catalog_unchanged_best_effort(run_id, catalog)
            .await;
        Ok(())
    }

    async fn get_run_info(&self, run_id: &str) -> StorageResult<RunInfo> {
        codec::validate_input_id(run_id)?;
        let _lock = self.lock.read().await;
        self.read_run(run_id).await
    }

    async fn list_runs(&self, status_filter: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        let _lock = self.lock.read().await;
        let mut runs = Vec::new();
        for run_id in self.listed_run_ids().await? {
            let info = self.read_run(&run_id).await?;
            if status_filter
                .as_ref()
                .is_none_or(|filter| &info.status == filter)
            {
                runs.push(info);
            }
        }
        runs.sort_by(|left, right| {
            compare_summaries(&RunSummary::from(left), &RunSummary::from(right))
        });
        Ok(runs)
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<()> {
        codec::validate_input_id(run_id)?;
        let _lock = self.lock.write().await;
        let mut catalog = catalog::CatalogTransaction::begin(self).await?;
        let run_name = Self::run_name(run_id);
        let summary_name = Self::summary_name(run_id);
        self.directory.inspect_regular(&summary_name).await?;
        if self.directory.inspect_regular(&run_name).await? == FileState::Missing {
            // Finish cleanup after a crash that removed the authoritative
            // primary but left its derived sidecar behind.
            catalog.mark_dirty().await?;
            self.directory.remove_regular(&summary_name).await?;
            self.remove_from_catalog_best_effort(run_id, catalog).await;
            return Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            )));
        }
        // Remove the cache first. If deletion stops between the two commits,
        // listing derives the summary from the still-authoritative primary.
        catalog.mark_dirty().await?;
        self.directory.remove_regular(&summary_name).await?;
        if !self.directory.remove_regular(&run_name).await? {
            return Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            )));
        }
        self.remove_from_catalog_best_effort(run_id, catalog).await;
        Ok(())
    }

    async fn list_run_summaries(
        &self,
        status_filter: Option<RunStatus>,
    ) -> StorageResult<Vec<RunSummary>> {
        let _lock = self.lock.read().await;
        let mut summaries = Vec::new();
        for run_id in self.listed_run_ids().await? {
            let summary = self.read_current_summary(&run_id).await?;
            if status_filter
                .as_ref()
                .is_none_or(|filter| &summary.status == filter)
            {
                summaries.push(summary);
            }
        }
        summaries.sort_by(compare_summaries);
        Ok(summaries)
    }

    async fn list_run_summaries_page(&self, query: &RunListQuery) -> StorageResult<RunSummaryPage> {
        let _lock = self.lock.read().await;
        catalog::list_page(self, query).await
    }
}
