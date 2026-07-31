use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::JsonStateStore;
use super::fs::FileState;
use crate::engine::types::{Context, RunStatus, TaskState};
use crate::storage::{RunLease, StateStore, StorageError, StorageResult};

pub(super) const LEASE_SUFFIX: &str = ".lease";
const MAX_OWNER_BYTES: usize = 128;

#[cfg(test)]
mod tests;

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct LeaseRecord {
    pub(super) run_id: String,
    pub(super) owner: String,
    pub(super) expires_micros: i64,
}

impl JsonStateStore {
    pub(super) async fn delete_run_with_lease_lock(&self, run_id: &str) -> StorageResult<()> {
        let run_id = run_id.to_string();
        self.with_lease_lock(move |store| async move {
            let _lock = store.lock.write().await;
            let mut catalog = super::catalog::CatalogTransaction::begin(&store).await?;
            let run_name = Self::run_name(&run_id);
            let summary_name = Self::summary_name(&run_id);
            store.directory.inspect_regular(&summary_name).await?;
            if store.directory.inspect_regular(&run_name).await? == FileState::Missing {
                catalog.mark_dirty().await?;
                store.directory.remove_regular(&summary_name).await?;
                store
                    .remove_from_catalog_best_effort(&run_id, catalog)
                    .await;
                store
                    .run_leases
                    .remove_regular(&lease_name(&run_id))
                    .await?;
                return Err(StorageError::not_found(format_args!(
                    "Run '{run_id}' not found"
                )));
            }
            let info = store.read_run(&run_id).await?;
            if !info.status.is_terminal()
                && store
                    .read_lease(&run_id)
                    .await?
                    .is_some_and(|lease| lease.expires_micros > Utc::now().timestamp_micros())
            {
                return Err(StorageError::conflict(format_args!(
                    "Run '{run_id}' is still executing"
                )));
            }
            catalog.mark_dirty().await?;
            store.directory.remove_regular(&summary_name).await?;
            if !store.directory.remove_regular(&run_name).await? {
                return Err(StorageError::not_found(format_args!(
                    "Run '{run_id}' not found"
                )));
            }
            store
                .remove_from_catalog_best_effort(&run_id, catalog)
                .await;
            store
                .run_leases
                .remove_regular(&lease_name(&run_id))
                .await?;
            Ok(())
        })
        .await
    }

    pub(super) async fn init_run_with_lease(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
        lease: &RunLease,
    ) -> StorageResult<()> {
        validate_lease(run_id, lease)?;
        let run_id = run_id.to_string();
        let flow_name = flow_name.to_string();
        let ctx = ctx.clone();
        let lease = lease.clone();
        self.with_lease_lock(move |store| async move {
            let name = lease_name(&run_id);
            let data = encode_lease(&run_id, &lease)?;
            store
                .run_leases
                .write_new(&name, &data, "run ownership lease")
                .await?;
            if let Err(error) = store.init_run(&run_id, &flow_name, &ctx).await {
                let _ = store.run_leases.remove_regular(&name).await;
                return Err(error);
            }
            Ok(())
        })
        .await
    }

    pub(super) async fn renew_lease_file(
        &self,
        run_id: &str,
        lease: &RunLease,
    ) -> StorageResult<bool> {
        validate_lease(run_id, lease)?;
        let run_id = run_id.to_string();
        let lease = lease.clone();
        self.with_lease_lock(move |store| async move {
            let Some(current) = store.read_lease(&run_id).await? else {
                return Ok(false);
            };
            if current.owner != lease.owner()
                || current.expires_micros <= Utc::now().timestamp_micros()
            {
                return Ok(false);
            }
            let info = store.get_run_info(&run_id).await?;
            if info.status.is_terminal() {
                store
                    .run_leases
                    .remove_regular(&lease_name(&run_id))
                    .await?;
                return Ok(false);
            }
            store
                .run_leases
                .write_replace(&lease_name(&run_id), &encode_lease(&run_id, &lease)?)
                .await?;
            Ok(true)
        })
        .await
    }

    pub(super) async fn set_status_with_lease(
        &self,
        run_id: &str,
        status: RunStatus,
        owner: &str,
    ) -> StorageResult<bool> {
        let run_id = run_id.to_string();
        let owner = owner.to_string();
        self.with_lease_lock(move |store| async move {
            let Some(current) = store.read_lease(&run_id).await? else {
                return Ok(false);
            };
            if current.owner != owner || current.expires_micros <= Utc::now().timestamp_micros() {
                return Ok(false);
            }
            let terminal = status.is_terminal();
            store.set_run_status(&run_id, status).await?;
            if terminal {
                store
                    .run_leases
                    .remove_regular(&lease_name(&run_id))
                    .await?;
            }
            Ok(true)
        })
        .await
    }

    pub(super) async fn upsert_task_with_lease(
        &self,
        run_id: &str,
        task: &TaskState,
        owner: &str,
    ) -> StorageResult<bool> {
        let run_id = run_id.to_string();
        let task = task.clone();
        let owner = owner.to_string();
        self.with_lease_lock(move |store| async move {
            if !store.lease_is_live(&run_id, &owner).await? {
                return Ok(false);
            }
            #[cfg(test)]
            store.pause_before_owned_commit().await;
            store.upsert_task(&run_id, &task).await?;
            Ok(true)
        })
        .await
    }

    pub(super) async fn update_ctx_with_lease(
        &self,
        run_id: &str,
        ctx: &Context,
        owner: &str,
    ) -> StorageResult<bool> {
        let run_id = run_id.to_string();
        let ctx = ctx.clone();
        let owner = owner.to_string();
        self.with_lease_lock(move |store| async move {
            if !store.lease_is_live(&run_id, &owner).await? {
                return Ok(false);
            }
            store.update_ctx(&run_id, &ctx).await?;
            Ok(true)
        })
        .await
    }

    #[cfg(test)]
    async fn pause_before_owned_commit(&self) {
        let hook = self.lease_commit_hook.lock().unwrap().take();
        if let Some((reached, resume)) = hook {
            reached.notify_one();
            resume.notified().await;
        }
    }

    async fn read_lease(&self, run_id: &str) -> StorageResult<Option<LeaseRecord>> {
        let name = lease_name(run_id);
        if self.run_leases.inspect_regular(&name).await? == FileState::Missing {
            return Ok(None);
        }
        self.read_named_lease(&name).await.map(Some)
    }

    async fn lease_is_live(&self, run_id: &str, owner: &str) -> StorageResult<bool> {
        Ok(self.read_lease(run_id).await?.is_some_and(|lease| {
            lease.owner == owner && lease.expires_micros > Utc::now().timestamp_micros()
        }))
    }

    pub(super) async fn read_named_lease(&self, name: &str) -> StorageResult<LeaseRecord> {
        let data =
            self.run_leases.read_regular(name).await?.ok_or_else(|| {
                StorageError::not_found("Run lease disappeared during inspection")
            })?;
        let record: LeaseRecord = serde_json::from_slice(&data)
            .map_err(|error| StorageError::corruption("Invalid JSON run lease", error))?;
        if lease_name(&record.run_id) != name || record.owner.is_empty() {
            return Err(StorageError::corruption(
                "Invalid JSON run lease",
                "lease identity does not match its file name",
            ));
        }
        Ok(record)
    }
}

pub(super) fn lease_name(run_id: &str) -> String {
    format!("{run_id}{LEASE_SUFFIX}")
}

fn validate_lease(run_id: &str, lease: &RunLease) -> StorageResult<()> {
    crate::storage::validate_run_id(run_id)
        .map_err(|error| StorageError::backend("Invalid run lease id", error))?;
    if lease.owner().is_empty() || lease.owner().len() > MAX_OWNER_BYTES {
        return Err(StorageError::backend(
            "Invalid run lease owner",
            "owner must contain 1 to 128 bytes",
        ));
    }
    Ok(())
}

fn encode_lease(run_id: &str, lease: &RunLease) -> StorageResult<Vec<u8>> {
    serde_json::to_vec(&LeaseRecord {
        run_id: run_id.to_string(),
        owner: lease.owner().to_string(),
        expires_micros: lease.expires_micros(),
    })
    .map_err(|error| StorageError::backend("Failed to serialize run lease", error))
}
