use crate::engine::types::{RunInfo, RunSummary};
use crate::storage::{StorageError, StorageResult};
use tracing::warn;

use super::CATALOG_NAME;
use super::format::{self, CatalogRecord, RECORD_BYTES};
use super::header::{self, HEADER_BYTES};
use super::state::{self, CatalogLock};
use crate::storage::json_store::JsonStateStore;
use crate::storage::json_store::codec;
use crate::storage::json_store::fs::SecureStoreDir;

pub(in crate::storage::json_store) struct CatalogTransaction<'store> {
    store: &'store JsonStateStore,
    directory: &'store SecureStoreDir,
    records: Option<Vec<CatalogRecord>>,
    generation: Option<uuid::Uuid>,
    _lock: CatalogLock,
}

impl<'store> CatalogTransaction<'store> {
    pub(in crate::storage::json_store) async fn begin(
        store: &'store JsonStateStore,
    ) -> StorageResult<Self> {
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
                    records: None,
                    generation: None,
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
            records: None,
            generation: Some(token.generation()),
            _lock: lock,
        })
    }

    pub(in crate::storage::json_store) async fn mark_dirty(&mut self) -> StorageResult<()> {
        state::mark_dirty(self.directory).await?;
        Ok(())
    }

    pub(in crate::storage::json_store) async fn upsert(
        &mut self,
        record: CatalogRecord,
    ) -> StorageResult<()> {
        let records = self.records().await?;
        records.retain(|existing| existing.id != record.id);
        let position = records
            .binary_search_by(|existing| format::compare_records(existing, &record))
            .unwrap_or_else(|position| position);
        records.insert(position, record);
        Ok(())
    }

    pub(in crate::storage::json_store) async fn remove(
        &mut self,
        run_id: &str,
    ) -> StorageResult<()> {
        self.records().await?.retain(|record| record.id != run_id);
        Ok(())
    }

    pub(in crate::storage::json_store) async fn commit(mut self) -> StorageResult<()> {
        let records = self.records.take().ok_or_else(|| {
            StorageError::corruption(
                "Invalid JSON run catalog transaction",
                "ordered projection was not loaded",
            )
        })?;
        write_records(self.directory, records).await?;
        Ok(())
    }

    async fn records(&mut self) -> StorageResult<&mut Vec<CatalogRecord>> {
        if self.records.is_none() {
            let records = match read_records(self.directory).await {
                Ok((_, records)) => records,
                Err(_) => {
                    rebuild_locked(self.store).await?;
                    let (generation, records) = read_records(self.directory).await?;
                    self.generation = Some(generation);
                    records
                }
            };
            self.records = Some(records);
        }
        Ok(self.records.as_mut().expect("catalog records were loaded"))
    }

    pub(in crate::storage::json_store) async fn commit_unchanged(self) -> StorageResult<()> {
        let generation = self.generation.ok_or_else(|| {
            StorageError::corruption(
                "Invalid JSON run catalog transaction",
                "catalog generation is unavailable",
            )
        })?;
        state::mark_clean(self.directory, generation).await?;
        Ok(())
    }
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
    /// missing, dirty, or malformed catalog is also rebuilt lazily by paging.
    pub async fn rebuild_run_summary_catalog(&self) -> StorageResult<usize> {
        let _local = self.lock.write().await;
        self.directory.ensure_created().await?;
        let _catalog = state::acquire_lock(&self.directory).await?;
        rebuild_locked(self).await
    }
}

pub(super) async fn ensure_current(store: &JsonStateStore) -> StorageResult<()> {
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
    write_records(&store.directory, records).await?;
    Ok(count)
}

async fn read_records(
    directory: &SecureStoreDir,
) -> StorageResult<(uuid::Uuid, Vec<CatalogRecord>)> {
    let data = directory.read_regular(CATALOG_NAME).await?.ok_or_else(|| {
        StorageError::corruption("Invalid JSON run catalog", "catalog is missing")
    })?;
    let header = header::decode(&data[..data.len().min(HEADER_BYTES)], data.len() as u64)?;
    let (_, count) = header.section(0)?;
    let count = usize::try_from(count)
        .map_err(|error| StorageError::corruption("Invalid JSON run catalog", error))?;
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let start = HEADER_BYTES + index * RECORD_BYTES;
        records.push(format::decode_record(&data[start..start + RECORD_BYTES])?);
    }
    for pair in records.windows(2) {
        if format::compare_records(&pair[0], &pair[1]) != std::cmp::Ordering::Less {
            return Err(StorageError::corruption(
                "Invalid JSON run catalog",
                "global section is unordered or duplicated",
            ));
        }
    }
    Ok((header.generation, records))
}

async fn write_records(
    directory: &SecureStoreDir,
    mut records: Vec<CatalogRecord>,
) -> StorageResult<()> {
    let (generation, data) = format::encode(records.as_mut_slice())?;
    directory.write_replace(CATALOG_NAME, &data).await?;
    state::mark_clean(directory, generation).await?;
    Ok(())
}
