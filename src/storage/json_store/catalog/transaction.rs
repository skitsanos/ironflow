use crate::engine::types::{RunInfo, RunSummary};
use crate::storage::{StorageError, StorageResult};
use tracing::warn;

use super::delta::DeltaEntry;
use super::format::{self, CatalogRecord};
use super::state::{self, CatalogLock, CatalogToken};
use crate::storage::json_store::JsonStateStore;
use crate::storage::json_store::codec;
use crate::storage::json_store::fs::SecureStoreDir;

mod persistence;

pub(in crate::storage::json_store) struct CatalogTransaction<'store> {
    store: &'store JsonStateStore,
    directory: &'store SecureStoreDir,
    token: Option<CatalogToken>,
    mutation: Option<DeltaEntry>,
    _lock: CatalogLock,
}

impl<'store> CatalogTransaction<'store> {
    pub(in crate::storage::json_store) async fn begin(
        store: &'store JsonStateStore,
    ) -> StorageResult<Self> {
        // Create control directories before a clean token fingerprints the run
        // root. Later schedule claims and lease heartbeats mutate only those
        // children and cannot invalidate the catalog (IF-062).
        store.ensure_control_directories().await?;
        let lock = state::acquire_lock(&store.directory).await?;
        let mut token = None;
        for attempt in 0..=3 {
            if let Some(current) = state::current_token(&store.directory).await? {
                token = Some(current);
                break;
            }
            if attempt == 3 {
                break;
            }
            if let Err(error) = rebuild_locked(store).await {
                warn!(
                    error = %error,
                    "JSON run catalog is unavailable; preserving primary-run mutation isolation"
                );
                return Ok(Self {
                    store,
                    directory: &store.directory,
                    token: None,
                    mutation: None,
                    _lock: lock,
                });
            }
            #[cfg(test)]
            store.wait_catalog_rebuild_hook().await;
        }
        let token = token.ok_or_else(|| {
            StorageError::conflict("JSON run catalog changed repeatedly while preparing a write")
        })?;
        Ok(Self {
            store,
            directory: &store.directory,
            token: Some(token),
            mutation: None,
            _lock: lock,
        })
    }

    pub(in crate::storage::json_store) async fn mark_dirty(&mut self) -> StorageResult<()> {
        state::mark_dirty(self.directory).await
    }

    pub(in crate::storage::json_store) async fn upsert(
        &mut self,
        record: CatalogRecord,
    ) -> StorageResult<()> {
        self.mutation = Some(DeltaEntry::Upsert(record));
        Ok(())
    }

    pub(in crate::storage::json_store) async fn remove(
        &mut self,
        run_id: &str,
    ) -> StorageResult<()> {
        self.mutation = Some(DeltaEntry::Delete(run_id.to_string()));
        Ok(())
    }

    pub(in crate::storage::json_store) async fn commit(mut self) -> StorageResult<()> {
        let mutation = self.mutation.take().ok_or_else(|| {
            StorageError::corruption(
                "Invalid JSON run catalog transaction",
                "projection mutation was not prepared",
            )
        })?;
        let token = self.token.as_ref().ok_or_else(unavailable_token)?;
        persistence::commit_mutation(self.store, token, mutation).await
    }

    pub(in crate::storage::json_store) async fn commit_unchanged(self) -> StorageResult<()> {
        let token = self.token.as_ref().ok_or_else(unavailable_token)?;
        state::mark_clean(
            self.directory,
            token.base_generation(),
            token.delta_revision(),
        )
        .await?;
        Ok(())
    }
}

fn unavailable_token() -> StorageError {
    StorageError::corruption(
        "Invalid JSON run catalog transaction",
        "catalog token is unavailable",
    )
}

impl JsonStateStore {
    pub(super) async fn catalog_record(&self, run_id: &str) -> StorageResult<CatalogRecord> {
        let record = self.read_run_record(run_id).await?;
        let summary = RunSummary::from(&record.info);
        if let (Some(revision), Some(digest)) =
            (record.revision.as_deref(), record.summary_digest.as_deref())
        {
            let sidecar = self.read_summary(run_id).await?;
            let current = sidecar.as_ref().is_some_and(|sidecar| {
                sidecar.revision.as_deref() == Some(revision)
                    && sidecar.digest.as_deref() == Some(digest)
            });
            if !current
                || !codec::summary_matches_digest(
                    &sidecar.expect("current sidecar exists").summary,
                    digest,
                    run_id,
                )?
            {
                self.repair_summary_best_effort(run_id, revision, &summary)
                    .await;
            }
        }
        CatalogRecord::from_summary(&summary)
    }

    pub(in crate::storage::json_store) async fn encoded_catalog_record(
        &self,
        info: &RunInfo,
        _encoded: &codec::EncodedRecord,
    ) -> StorageResult<CatalogRecord> {
        CatalogRecord::from_summary(&RunSummary::from(info))
    }

    pub(super) async fn rebuild_catalog_locked(&self) -> StorageResult<usize> {
        rebuild_locked(self).await
    }

    /// Rebuild the derived ordered catalog from authoritative run records.
    ///
    /// Call this while writers are stopped for an explicit offline repair. A
    /// missing, dirty, or malformed base or delta is also rebuilt lazily by
    /// paging. Rebuilding compacts and resets the bounded mutation delta.
    pub async fn rebuild_run_summary_catalog(&self) -> StorageResult<usize> {
        let _local = self.lock.write().await;
        self.directory.ensure_created().await?;
        self.ensure_control_directories().await?;
        let _catalog = state::acquire_lock(&self.directory).await?;
        rebuild_locked(self).await
    }
}

pub(super) async fn ensure_current(store: &JsonStateStore) -> StorageResult<()> {
    store.ensure_control_directories().await?;
    if state::current_token(&store.directory).await?.is_some() {
        return Ok(());
    }
    let _lock = state::acquire_lock(&store.directory).await?;
    if state::current_token(&store.directory).await?.is_none() {
        rebuild_locked(store).await?;
    }
    Ok(())
}

async fn rebuild_locked(store: &JsonStateStore) -> StorageResult<usize> {
    state::mark_dirty(&store.directory).await?;
    let run_ids = store.listed_run_ids().await?;
    let mut records = Vec::with_capacity(run_ids.len());
    for run_id in run_ids {
        records.push(store.catalog_record(&run_id).await?);
    }
    records.sort_by(format::compare_records);
    let count = records.len();
    persistence::replace_snapshot(store, records, false).await?;
    Ok(count)
}
