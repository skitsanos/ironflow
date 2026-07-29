//! Cross-process schedule claims backed by exclusive file creation.
//!
//! Single-host by nature, but two processes sharing one `store_dir` still
//! coordinate correctly: the claim commits through the same no-follow secure
//! directory layer the run records use, and losing the create means another
//! process already owns the instant.

use std::time::{Duration, SystemTime};

use super::JsonStateStore;
use crate::storage::{StorageErrorKind, StorageResult};

/// Distinguishes claim files from run records. Deliberately not `*.json`:
/// the directory scan treats every `*.json` entry as a run record, so a claim
/// using that extension would corrupt run listings.
const CLAIM_PREFIX: &str = ".ironflow-schedule-claim-v1.";

impl JsonStateStore {
    /// File name for one claim.
    ///
    /// Hex-encoding the `name`/`key` pair keeps the mapping injective and the
    /// result filesystem-safe regardless of what an operator names a schedule.
    fn claim_name(name: &str, key: &str) -> String {
        let identity = format!("{name}\u{0}{key}");
        format!("{CLAIM_PREFIX}{}", hex::encode(identity.as_bytes()))
    }

    pub(super) async fn claim_schedule_file(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> StorageResult<bool> {
        let _lock = self.lock.write().await;
        self.directory.ensure_created().await?;
        self.reap_expired_claims(ttl_seconds).await;

        let file = Self::claim_name(name, key);
        match self
            .directory
            .write_new(&file, key.as_bytes(), "schedule claim")
            .await
        {
            Ok(()) => Ok(true),
            // The commit is atomic, so a conflict means a peer got there first.
            Err(error) if error.kind() == StorageErrorKind::Conflict => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Drop claim files older than the TTL.
    ///
    /// Best-effort: a claim that outlives its window only wastes an inode, and
    /// failing a fire because cleanup failed would be worse than the leak.
    /// Runs on the claim path itself because nothing in `serve` drives run
    /// retention, so there is no periodic sweep to attach to.
    async fn reap_expired_claims(&self, ttl_seconds: u64) {
        let Ok(entries) = self.directory.list_entries().await else {
            return;
        };
        let ttl = Duration::from_secs(ttl_seconds);
        let now = SystemTime::now();

        for entry in entries {
            let Some(entry_name) = entry.name.to_str() else {
                continue;
            };
            if !entry_name.starts_with(CLAIM_PREFIX) {
                continue;
            }
            let expired = tokio::fs::symlink_metadata(self.directory.path(entry_name))
                .await
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age >= ttl);
            if expired {
                let _ = self.directory.remove_regular(entry_name).await;
            }
        }
    }
}
