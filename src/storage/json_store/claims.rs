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
    /// `name` and `key` are hex-encoded into their own segments, joined by a
    /// literal `.`. Hex output is `[0-9a-f]` only, so it can never itself
    /// contain a `.`; that makes the segment boundary unambiguous and the
    /// mapping injective regardless of what either input contains — no
    /// dependency on `name` or `key` being NUL-free or otherwise restricted.
    /// The scheme also lets `reap_expired_claims` match on the `name` segment
    /// alone, so it can scope a reap to one schedule's own claims.
    fn claim_name(name: &str, key: &str) -> String {
        format!(
            "{}{}",
            Self::claim_prefix_for(name),
            hex::encode(key.as_bytes())
        )
    }

    /// Prefix shared by every claim file belonging to `name`, hex segment
    /// included. Used both to build a full claim file name and to scope
    /// reaping to one schedule's own entries.
    fn claim_prefix_for(name: &str) -> String {
        format!("{CLAIM_PREFIX}{}.", hex::encode(name.as_bytes()))
    }

    pub(super) async fn claim_schedule_file(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> StorageResult<bool> {
        let _lock = self.lock.write().await;
        self.directory.ensure_created().await?;
        self.reap_expired_claims(name, ttl_seconds).await;

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

    /// Drop `name`'s own claim files older than the TTL.
    ///
    /// Best-effort: a claim that outlives its window only wastes an inode, and
    /// failing a fire because cleanup failed would be worse than the leak.
    /// Runs on the claim path itself because nothing in `serve` drives run
    /// retention, so there is no periodic sweep to attach to.
    ///
    /// Scoped to `name`'s own prefix rather than every `CLAIM_PREFIX` entry:
    /// `ttl_seconds` is this call's schedule's TTL, derived from that
    /// schedule's own `grace_seconds`. Applying it to another schedule's
    /// claims would reap a still-valid long-TTL claim on a short-TTL
    /// schedule's routine call, letting a restarted process re-fire an
    /// instant it had already claimed.
    async fn reap_expired_claims(&self, name: &str, ttl_seconds: u64) {
        let Ok(entries) = self.directory.list_entries().await else {
            return;
        };
        let prefix = Self::claim_prefix_for(name);
        let ttl = Duration::from_secs(ttl_seconds);
        let now = SystemTime::now();

        for entry in entries {
            let Some(entry_name) = entry.name.to_str() else {
                continue;
            };
            if !entry_name.starts_with(&prefix) {
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
