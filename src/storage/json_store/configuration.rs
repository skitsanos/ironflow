use std::path::Path;

use tokio::sync::RwLock;

use super::JsonStateStore;
use super::fs::SecureStoreDir;
use crate::storage::StorageResult;

impl JsonStateStore {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        let base_dir = base_dir.as_ref().to_path_buf();
        Self {
            schedule_claims: SecureStoreDir::new(base_dir.join(".ironflow-schedule-claims-v1")),
            run_leases: SecureStoreDir::new(base_dir.join(".ironflow-run-leases-v1")),
            directory: SecureStoreDir::new(base_dir),
            lock: std::sync::Arc::new(RwLock::new(())),
            #[cfg(test)]
            fail_next_summary_commit: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
            #[cfg(test)]
            directory_entries_examined: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            #[cfg(test)]
            current_summary_reads: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            #[cfg(test)]
            catalog_io: std::sync::Arc::new(super::test_support::CatalogIoCounters::default()),
            #[cfg(test)]
            catalog_read_hook: std::sync::Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            catalog_rebuild_hook: std::sync::Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            lease_reap_hook: std::sync::Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            lease_commit_hook: std::sync::Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            lease_lock_attempt_hook: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub(super) async fn ensure_control_directories(&self) -> StorageResult<()> {
        self.schedule_claims.ensure_created().await?;
        self.run_leases.ensure_created().await
    }
}
