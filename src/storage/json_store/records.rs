#[cfg(test)]
use std::sync::atomic::Ordering;

use tracing::warn;

use crate::engine::types::{RunInfo, RunSummary};
use crate::storage::{StorageError, StorageResult};

use super::JsonStateStore;
use super::catalog::{CatalogRecord, CatalogTransaction};
use super::codec;
use super::fs::FileState;

impl JsonStateStore {
    pub(super) fn run_name(run_id: &str) -> String {
        format!("{run_id}.json")
    }

    pub(super) fn summary_name(run_id: &str) -> String {
        format!("{run_id}.summary.json")
    }

    pub(super) async fn read_run_record(&self, run_id: &str) -> StorageResult<codec::DecodedRun> {
        codec::validate_input_id(run_id)?;
        let name = Self::run_name(run_id);
        let data = self
            .directory
            .read_regular(&name)
            .await?
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))?;
        codec::decode_run(&data, run_id, &name)
    }

    pub(super) async fn read_run(&self, run_id: &str) -> StorageResult<RunInfo> {
        Ok(self.read_run_record(run_id).await?.info)
    }

    pub(super) async fn write_existing_run(
        &self,
        catalog: &mut CatalogTransaction<'_>,
        run_id: &str,
        info: &RunInfo,
    ) -> StorageResult<CatalogRecord> {
        let run_name = Self::run_name(run_id);
        if self.directory.inspect_regular(&run_name).await? == FileState::Missing {
            return Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            )));
        }
        self.preflight_summary(run_id).await?;
        let encoded = codec::encode_record(info, run_id)?;
        catalog.mark_dirty().await?;
        self.directory
            .write_replace(&run_name, &encoded.run)
            .await?;
        self.write_summary_best_effort(run_id, &encoded).await;
        self.encoded_catalog_record(info, &encoded).await
    }

    pub(super) async fn write_new_run(
        &self,
        catalog: &mut CatalogTransaction<'_>,
        run_id: &str,
        info: &RunInfo,
    ) -> StorageResult<CatalogRecord> {
        self.preflight_summary(run_id).await?;
        let encoded = codec::encode_record(info, run_id)?;
        catalog.mark_dirty().await?;
        self.directory
            .write_new(&Self::run_name(run_id), &encoded.run, run_id)
            .await?;
        self.write_summary_best_effort(run_id, &encoded).await;
        self.encoded_catalog_record(info, &encoded).await
    }

    async fn preflight_summary(&self, run_id: &str) -> StorageResult<()> {
        self.directory
            .inspect_regular(&Self::summary_name(run_id))
            .await?;
        Ok(())
    }

    async fn write_summary_best_effort(&self, run_id: &str, encoded: &codec::EncodedRecord) {
        if let Err(error) = self.write_summary(run_id, &encoded.summary).await {
            warn!(
                run_id,
                revision = encoded.revision,
                error = %error,
                "failed to commit JSON summary cache; the primary run remains authoritative"
            );
        }
    }

    async fn write_summary(&self, run_id: &str, data: &[u8]) -> StorageResult<()> {
        #[cfg(test)]
        if self.fail_next_summary_commit.swap(false, Ordering::SeqCst) {
            return Err(StorageError::backend(
                format_args!("Failed to replace summary for run '{run_id}'"),
                "injected summary commit failure",
            ));
        }
        self.directory
            .write_replace(&Self::summary_name(run_id), data)
            .await
    }

    pub(super) async fn repair_summary_best_effort(
        &self,
        run_id: &str,
        revision: &str,
        summary: &RunSummary,
    ) {
        let data = match codec::encode_summary(revision, summary, run_id) {
            Ok(data) => data,
            Err(error) => {
                warn!(run_id, revision, error = %error, "failed to encode replacement JSON summary cache");
                return;
            }
        };
        if let Err(error) = self.write_summary(run_id, &data).await {
            warn!(run_id, revision, error = %error, "failed to repair JSON summary cache; using the authoritative primary run");
        }
    }

    pub(super) async fn upsert_catalog_best_effort(
        &self,
        run_id: &str,
        mut catalog: CatalogTransaction<'_>,
        record: CatalogRecord,
    ) {
        let result = match catalog.upsert(record).await {
            Ok(()) => catalog.commit().await,
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            warn!(run_id, error = %error, "failed to commit JSON run catalog; the primary run remains authoritative");
        }
    }

    pub(super) async fn remove_from_catalog_best_effort(
        &self,
        run_id: &str,
        mut catalog: CatalogTransaction<'_>,
    ) {
        let result = match catalog.remove(run_id).await {
            Ok(()) => catalog.commit().await,
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            warn!(run_id, error = %error, "failed to remove JSON run catalog member; the primary store remains authoritative");
        }
    }

    pub(super) async fn commit_catalog_unchanged_best_effort(
        &self,
        run_id: &str,
        catalog: CatalogTransaction<'_>,
    ) {
        if let Err(error) = catalog.commit_unchanged().await {
            warn!(run_id, error = %error, "failed to confirm unchanged JSON run catalog; the primary run remains authoritative");
        }
    }

    pub(super) async fn listed_run_ids(&self) -> StorageResult<Vec<String>> {
        let mut run_ids = Vec::new();
        let Some(mut entries) = self.directory.stream_entries().await? else {
            return Ok(run_ids);
        };
        while let Some(entry) = entries.next().await? {
            #[cfg(test)]
            self.directory_entries_examined
                .fetch_add(1, Ordering::Relaxed);
            let Some(name) = entry.name.to_str() else {
                continue;
            };
            let Some((run_id, is_summary)) = codec::managed_entry(name) else {
                continue;
            };
            codec::validate_filename_id(run_id, name)?;
            if !entry.file_type.is_file() {
                return Err(StorageError::corruption(
                    format_args!("Unsafe JSON store entry '{name}'"),
                    "matching entry is not a regular file",
                ));
            }
            if !is_summary {
                run_ids.push(run_id.to_string());
            }
        }
        Ok(run_ids)
    }

    pub(super) async fn read_summary(
        &self,
        run_id: &str,
    ) -> StorageResult<Option<codec::DecodedSummary>> {
        let name = Self::summary_name(run_id);
        let Some(data) = self.directory.read_regular(&name).await? else {
            return Ok(None);
        };
        codec::decode_summary(&data, run_id, &name)
    }

    pub(super) async fn read_current_summary(&self, run_id: &str) -> StorageResult<RunSummary> {
        #[cfg(test)]
        self.current_summary_reads.fetch_add(1, Ordering::Relaxed);
        let run_name = Self::run_name(run_id);
        let prefix = self
            .directory
            .read_regular_prefix(&run_name, codec::REVISION_PREFIX_BYTES)
            .await?
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))?;
        if let Some(primary) = codec::decode_revision_prefix(&prefix, run_id)
            && let Some(sidecar) = self.read_summary(run_id).await?
            && sidecar.revision.as_deref() == Some(primary.revision.as_str())
            && sidecar.digest.as_deref() == Some(primary.digest.as_str())
            && codec::summary_matches_digest(&sidecar.summary, &primary.digest, run_id)?
        {
            return Ok(sidecar.summary);
        }
        let record = self.read_run_record(run_id).await?;
        let summary = RunSummary::from(&record.info);
        let sidecar = self.read_summary(run_id).await?;
        if let (Some(revision), Some(digest)) =
            (record.revision.as_deref(), record.summary_digest.as_deref())
        {
            if let Some(sidecar) = sidecar
                && sidecar.revision.as_deref() == Some(revision)
                && sidecar.digest.as_deref() == Some(digest)
                && codec::summary_matches_digest(&sidecar.summary, digest, run_id)?
            {
                return Ok(summary);
            }
            self.repair_summary_best_effort(run_id, revision, &summary)
                .await;
        }
        Ok(summary)
    }
}
