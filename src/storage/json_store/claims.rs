//! Cross-process schedule claims backed by exclusive file creation.
//!
//! The original flat claim file remains the atomic coordination point so a
//! rolling deployment can mix binaries safely. A separate digest-sharded,
//! time-bucketed index makes retention local to one schedule without changing
//! that claim identity.

use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest as _, Sha256};

use super::JsonStateStore;
use super::fs::SecureStoreDir;
use crate::storage::{StorageErrorKind, StorageResult};

mod cleanup;

const CLAIM_PREFIX: &str = ".ironflow-schedule-claim-v1.";
const INDEX_BUCKET_SECONDS: u64 = 3_600;
const LEGACY_INDEX_COMPLETE: &str = ".legacy-v1-index-complete";

impl JsonStateStore {
    fn claim_name(name: &str, key: &str) -> String {
        format!(
            "{}{}",
            Self::claim_prefix_for(name),
            hex::encode(key.as_bytes())
        )
    }

    fn claim_prefix_for(name: &str) -> String {
        format!("{CLAIM_PREFIX}{}.", hex::encode(name.as_bytes()))
    }

    fn schedule_index_directory(&self, name: &str) -> SecureStoreDir {
        let digest = hex::encode(Sha256::digest(name.as_bytes()));
        SecureStoreDir::new(self.schedule_claim_index.path(&digest))
    }

    fn claim_bucket_directory(&self, name: &str, claimed_at: SystemTime) -> SecureStoreDir {
        let seconds = claimed_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let bucket = format!("{:016x}", seconds / INDEX_BUCKET_SECONDS);
        SecureStoreDir::new(self.schedule_index_directory(name).path(&bucket))
    }

    fn index_marker_name(key: &str) -> String {
        hex::encode(Sha256::digest(key.as_bytes()))
    }

    pub(super) async fn claim_schedule_file(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> StorageResult<bool> {
        self.schedule_claims.ensure_created().await?;
        if self.schedule_cleanup.should_run(name, ttl_seconds).await {
            self.reap_expired_claims(name, ttl_seconds).await;
        }

        let file = Self::claim_name(name, key);
        let claimed = match self
            .schedule_claims
            .write_new(&file, key.as_bytes(), "schedule claim")
            .await
        {
            Ok(()) => true,
            // The commit is atomic, so a conflict means a peer got there first.
            Err(error) if error.kind() == StorageErrorKind::Conflict => false,
            Err(error) => return Err(error),
        };

        // Indexing is retention metadata, not part of claim ownership. A
        // failure may retain the claim longer, but must never turn a committed
        // claim into a reported failure or allow another replica to win it.
        self.index_claim_best_effort(name, key, &file).await;
        Ok(claimed)
    }

    async fn index_claim_best_effort(&self, name: &str, key: &str, claim_file: &str) {
        let schedule_dir = self.schedule_index_directory(name);
        let bucket = self.claim_bucket_directory(name, SystemTime::now());
        let marker = Self::index_marker_name(key);
        let result = async {
            self.schedule_claim_index.ensure_created().await?;
            schedule_dir.ensure_created().await?;
            bucket
                .write_new(&marker, claim_file.as_bytes(), "schedule claim index")
                .await
        }
        .await;
        if let Err(error) = result
            && error.kind() != StorageErrorKind::Conflict
        {
            tracing::debug!(
                error = %error,
                schedule = name,
                "schedule claim index update failed; claim remains authoritative"
            );
        }
    }
}
