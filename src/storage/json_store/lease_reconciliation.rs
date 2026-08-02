use chrono::{DateTime, Utc};

use super::JsonStateStore;
use super::leases::LEASE_SUFFIX;
use crate::engine::types::{RunStatus, TaskStatus};
use crate::storage::StorageResult;

impl JsonStateStore {
    pub(super) async fn reconcile_lease_files(&self, now: DateTime<Utc>) -> StorageResult<usize> {
        let mut reconciled = 0;
        let Some(mut entries) = self.run_leases.stream_entries().await? else {
            return Ok(0);
        };
        while let Some(entry) = entries.next().await? {
            let Some(name) = entry.name.to_str() else {
                continue;
            };
            if !name.ends_with(LEASE_SUFFIX) {
                continue;
            }
            let record = match self.read_named_lease(name).await {
                Ok(record) => record,
                Err(error) if error.is_not_found() => continue,
                Err(error) => return Err(error),
            };
            if record.expires_micros > now.timestamp_micros() {
                continue;
            }
            if self.reconcile_lease_candidate(name, now).await? {
                reconciled += 1;
                #[cfg(test)]
                self.pause_after_reaped_candidate().await;
            }
        }
        Ok(reconciled)
    }

    async fn reconcile_lease_candidate(
        &self,
        name: &str,
        now: DateTime<Utc>,
    ) -> StorageResult<bool> {
        let name = name.to_string();
        self.with_lease_lock(move |store| async move {
            let record = match store.read_named_lease(&name).await {
                Ok(record) if record.expires_micros <= now.timestamp_micros() => record,
                Ok(_) => return Ok(false),
                Err(error) if error.is_not_found() => return Ok(false),
                Err(error) => return Err(error),
            };
            let reconciled = match store.stall_expired_run(&record.run_id, now).await {
                Ok(reconciled) => reconciled,
                Err(error) if error.is_not_found() => false,
                Err(error) => return Err(error),
            };
            store.run_leases.remove_regular(&name).await?;
            Ok(reconciled)
        })
        .await
    }

    async fn stall_expired_run(
        &self,
        run_id: &str,
        finished: DateTime<Utc>,
    ) -> StorageResult<bool> {
        let _lock = self.lock.write().await;
        let mut catalog = super::catalog::CatalogTransaction::begin(self).await?;
        let mut info = self.read_run(run_id).await?;
        if info.status.is_terminal() {
            return Ok(false);
        }
        for task in info.tasks.values_mut() {
            if task.status.is_terminal() {
                continue;
            }
            task.status = if task.status == TaskStatus::Running {
                TaskStatus::Failed
            } else {
                TaskStatus::Skipped
            };
            task.error = Some("task stopped after execution-owner lease expired".to_string());
            task.finished = Some(finished);
        }
        info.status = RunStatus::Stalled;
        info.finished.get_or_insert(finished);
        let record = self.write_existing_run(&mut catalog, run_id, &info).await?;
        self.upsert_catalog_best_effort(run_id, catalog, record)
            .await;
        Ok(true)
    }

    #[cfg(test)]
    async fn pause_after_reaped_candidate(&self) {
        let hook = self.lease_reap_hook.lock().unwrap().take();
        if let Some((reached, resume)) = hook {
            reached.notify_one();
            resume.notified().await;
        }
    }
}
