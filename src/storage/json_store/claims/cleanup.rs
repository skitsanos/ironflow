use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest as _, Sha256};

use super::{INDEX_BUCKET_SECONDS, JsonStateStore, LEGACY_INDEX_COMPLETE};
use crate::storage::StorageErrorKind;
use crate::storage::schedule_cleanup::CLAIM_CLEANUP_BATCH_SIZE;

const MAX_INDEX_ENTRY_BYTES: usize = 1_024;

impl JsonStateStore {
    /// Delete a bounded batch of this schedule's expired indexed claims.
    ///
    /// Schedule and hour sharding means neither unrelated schedules nor live
    /// future buckets are inspected. Cleanup remains best-effort because a
    /// retained claim only costs storage; claim ownership must not depend on
    /// retention succeeding.
    pub(super) async fn reap_expired_claims(&self, name: &str, ttl_seconds: u64) {
        if let Err(error) = self.reap_expired_claim_batch(name, ttl_seconds).await {
            tracing::debug!(
                error = %error,
                schedule = name,
                "schedule claim cleanup failed; continuing"
            );
        }
    }

    async fn reap_expired_claim_batch(
        &self,
        name: &str,
        ttl_seconds: u64,
    ) -> crate::storage::StorageResult<()> {
        self.migrate_legacy_claim_batch().await?;
        let schedule_dir = self.schedule_index_directory(name);
        let Some(mut buckets) = schedule_dir.stream_entries().await? else {
            return Ok(());
        };
        let now = SystemTime::now();
        let latest_bucket = eligible_bucket(now, ttl_seconds);
        let prefix = Self::claim_prefix_for(name);
        let mut removed = 0;

        while removed < CLAIM_CLEANUP_BATCH_SIZE {
            let Some(entry) = buckets.next().await? else {
                break;
            };
            if !entry.file_type.is_dir() {
                continue;
            }
            let Some(bucket_name) = entry.name.to_str() else {
                continue;
            };
            let Ok(bucket_number) = u64::from_str_radix(bucket_name, 16) else {
                continue;
            };
            if bucket_number > latest_bucket {
                continue;
            }

            let bucket_dir = super::SecureStoreDir::new(schedule_dir.path(bucket_name));
            removed += self
                .reap_bucket(&bucket_dir, &prefix, ttl_seconds, now, removed)
                .await?;
            let _ = remove_if_empty(&bucket_dir).await;
        }
        drop(buckets);
        let _ = remove_if_empty(&schedule_dir).await;
        Ok(())
    }

    /// Populate the cleanup index for flat claims written before IF-075.
    ///
    /// The authoritative file never moves. Migration writes at most one normal
    /// cleanup batch per pass, then records completion durably so steady-state
    /// retention never scans the flat rolling-upgrade namespace. One global
    /// pass populates every schedule shard, avoiding a full legacy scan for
    /// each configured schedule.
    async fn migrate_legacy_claim_batch(&self) -> crate::storage::StorageResult<()> {
        if self
            .schedule_claim_index
            .inspect_regular(LEGACY_INDEX_COMPLETE)
            .await?
            == super::super::fs::FileState::Regular
        {
            return Ok(());
        }
        let Some(mut claims) = self.schedule_claims.stream_entries().await? else {
            return self.mark_legacy_index_complete().await;
        };
        let mut migrated = 0;

        while let Some(entry) = claims.next().await? {
            if !entry.file_type.is_file() {
                continue;
            }
            let Some(claim_file) = entry.name.to_str() else {
                continue;
            };
            let Some((name, key)) = legacy_claim_identity(claim_file) else {
                continue;
            };
            let metadata =
                match tokio::fs::symlink_metadata(self.schedule_claims.path(claim_file)).await {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => {
                        return Err(crate::storage::StorageError::backend(
                            "Failed to inspect legacy schedule claim",
                            error,
                        ));
                    }
                };
            let modified = metadata.modified().map_err(|error| {
                crate::storage::StorageError::backend(
                    "Failed to inspect legacy schedule claim timestamp",
                    error,
                )
            })?;
            let schedule_dir = self.schedule_index_directory(&name);
            let bucket = self.claim_bucket_directory(&name, modified);
            let marker = hex::encode(Sha256::digest(&key));
            if bucket.inspect_regular(&marker).await? == super::super::fs::FileState::Regular {
                continue;
            }

            self.schedule_claim_index.ensure_created().await?;
            schedule_dir.ensure_created().await?;
            match bucket
                .write_new(&marker, claim_file.as_bytes(), "schedule claim index")
                .await
            {
                Ok(()) => migrated += 1,
                Err(error) if error.kind() == StorageErrorKind::Conflict => {}
                Err(error) => return Err(error),
            }
            if migrated == CLAIM_CLEANUP_BATCH_SIZE {
                return Ok(());
            }
        }
        drop(claims);
        self.mark_legacy_index_complete().await
    }

    async fn mark_legacy_index_complete(&self) -> crate::storage::StorageResult<()> {
        self.schedule_claim_index.ensure_created().await?;
        match self
            .schedule_claim_index
            .write_new(
                LEGACY_INDEX_COMPLETE,
                b"v1",
                "schedule claim index migration",
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == StorageErrorKind::Conflict => Ok(()),
            Err(error) => Err(error),
        }
    }

    async fn reap_bucket(
        &self,
        bucket: &super::SecureStoreDir,
        claim_prefix: &str,
        ttl_seconds: u64,
        now: SystemTime,
        already_removed: usize,
    ) -> crate::storage::StorageResult<usize> {
        let Some(mut entries) = bucket.stream_entries().await? else {
            return Ok(0);
        };
        let mut removed = 0;
        let remaining = CLAIM_CLEANUP_BATCH_SIZE - already_removed;

        while removed < remaining {
            let Some(entry) = entries.next().await? else {
                break;
            };
            if !entry.file_type.is_file() {
                continue;
            }
            let Some(marker) = entry.name.to_str() else {
                continue;
            };
            if !marker_is_expired(bucket, marker, ttl_seconds, now).await {
                continue;
            }
            let Some(claim_file) = indexed_claim_name(bucket, marker, claim_prefix).await else {
                continue;
            };

            // Remove the index first. A concurrent claimant that still sees
            // the old canonical claim recreates the marker; a claimant after
            // canonical deletion creates both records normally.
            if bucket.remove_regular(marker).await? {
                let _ = self.schedule_claims.remove_regular(&claim_file).await;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn eligible_bucket(now: SystemTime, ttl_seconds: u64) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(ttl_seconds)
        / INDEX_BUCKET_SECONDS
}

async fn marker_is_expired(
    bucket: &super::SecureStoreDir,
    marker: &str,
    ttl_seconds: u64,
    now: SystemTime,
) -> bool {
    tokio::fs::symlink_metadata(bucket.path(marker))
        .await
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age >= Duration::from_secs(ttl_seconds))
}

async fn indexed_claim_name(
    bucket: &super::SecureStoreDir,
    marker: &str,
    claim_prefix: &str,
) -> Option<String> {
    let raw = bucket
        .read_regular_prefix(marker, MAX_INDEX_ENTRY_BYTES + 1)
        .await
        .ok()??;
    if raw.len() > MAX_INDEX_ENTRY_BYTES {
        return None;
    }
    let name = std::str::from_utf8(&raw).ok()?;
    let suffix = name.strip_prefix(claim_prefix)?;
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(name.to_owned())
}

fn legacy_claim_identity(claim_file: &str) -> Option<(String, Vec<u8>)> {
    let encoded = claim_file.strip_prefix(super::CLAIM_PREFIX)?;
    let (encoded_name, encoded_key) = encoded.split_once('.')?;
    if encoded_name.is_empty() || encoded_key.is_empty() || encoded_key.contains('.') {
        return None;
    }
    let name = String::from_utf8(hex::decode(encoded_name).ok()?).ok()?;
    let key = hex::decode(encoded_key).ok()?;
    Some((name, key))
}

/// Remove a digest-derived index directory when a concurrent writer has not
/// repopulated it. The secure-directory inspection rejects symlinks first.
async fn remove_if_empty(directory: &super::SecureStoreDir) -> crate::storage::StorageResult<bool> {
    if !directory.exists().await? {
        return Ok(false);
    }
    let root = directory.path("");
    match tokio::fs::remove_dir(&root).await {
        Ok(()) => {
            if let Some(parent) = root.parent() {
                super::super::platform::sync_directory(parent).await?;
            }
            Ok(true)
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(crate::storage::StorageError::backend(
            "Failed to remove empty schedule-claim index directory",
            error,
        )),
    }
}
