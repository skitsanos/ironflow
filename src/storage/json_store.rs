use std::collections::HashMap;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize};

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use crate::engine::types::*;
use crate::storage::run_listing::compare_summaries;
use crate::storage::{RunListQuery, RunSummaryPage, StateStore, StorageError, StorageResult};

#[cfg(test)]
mod cancellation_tests;
mod catalog;
mod claims;
mod codec;
mod configuration;
mod fs;
mod lease_lock;
mod lease_reconciliation;
mod leases;
mod listing;
mod platform;
mod records;
#[cfg(test)]
mod revision_tests;
mod temp;
#[cfg(test)]
mod test_support;

use fs::{FileState, SecureStoreDir};

#[cfg(test)]
type PauseHook = Arc<Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>>;

/// File-based JSON state store. Each run is stored as a separate JSON file.
#[derive(Clone)]
pub struct JsonStateStore {
    directory: SecureStoreDir,
    schedule_claims: SecureStoreDir,
    run_leases: SecureStoreDir,
    lock: Arc<RwLock<()>>,
    #[cfg(test)]
    fail_next_summary_commit: Arc<AtomicBool>,
    #[cfg(test)]
    directory_entries_examined: Arc<AtomicUsize>,
    #[cfg(test)]
    current_summary_reads: Arc<AtomicUsize>,
    #[cfg(test)]
    catalog_io: Arc<test_support::CatalogIoCounters>,
    #[cfg(test)]
    catalog_read_hook: PauseHook,
    #[cfg(test)]
    catalog_rebuild_hook: PauseHook,
    #[cfg(test)]
    lease_reap_hook: PauseHook,
    #[cfg(test)]
    lease_commit_hook: PauseHook,
    #[cfg(test)]
    lease_lock_attempt_hook: Arc<Mutex<Option<Arc<tokio::sync::Notify>>>>,
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

    async fn init_run_owned(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
        lease: &crate::storage::RunLease,
    ) -> StorageResult<()> {
        self.init_run_with_lease(run_id, flow_name, ctx, lease)
            .await
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        codec::validate_input_id(run_id)?;
        let _lock = self.lock.write().await;
        let mut catalog = catalog::CatalogTransaction::begin(self).await?;
        let mut info = self.read_run(run_id).await?;
        let is_terminal = status.is_terminal();
        info.status = status;
        // Preserve the first terminal transition's timestamp; a repeated
        // terminal write must not move `finished` (IF-052).
        if is_terminal && info.finished.is_none() {
            info.finished = Some(Utc::now());
        }
        let record = self.write_existing_run(&mut catalog, run_id, &info).await?;
        self.upsert_catalog_best_effort(run_id, catalog, record)
            .await;
        Ok(())
    }

    async fn set_run_status_owned(
        &self,
        run_id: &str,
        status: RunStatus,
        owner: &str,
    ) -> StorageResult<bool> {
        self.set_status_with_lease(run_id, status, owner).await
    }

    async fn renew_run_lease(
        &self,
        run_id: &str,
        lease: &crate::storage::RunLease,
    ) -> StorageResult<bool> {
        self.renew_lease_file(run_id, lease).await
    }

    async fn reconcile_expired_run_leases(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> StorageResult<usize> {
        self.reconcile_lease_files(now).await
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

    async fn upsert_task_owned(
        &self,
        run_id: &str,
        task: &TaskState,
        owner: &str,
    ) -> StorageResult<bool> {
        self.upsert_task_with_lease(run_id, task, owner).await
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

    async fn update_ctx_owned(
        &self,
        run_id: &str,
        ctx: &Context,
        owner: &str,
    ) -> StorageResult<bool> {
        self.update_ctx_with_lease(run_id, ctx, owner).await
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
        self.delete_run_with_lease_lock(run_id).await
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

    /// Prune via bounded summary pages rather than the default full-catalog scan
    /// (IF-051).
    async fn prune_before(&self, cutoff: chrono::DateTime<chrono::Utc>) -> StorageResult<usize> {
        crate::storage::prune_before_via_summary_pages(self, cutoff).await
    }

    async fn claim_schedule(&self, name: &str, key: &str, ttl_seconds: u64) -> StorageResult<bool> {
        self.claim_schedule_file(name, key, ttl_seconds).await
    }
}
