use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, Weak};

use super::JsonStateStore;
use super::fs::SecureStoreDir;
use super::platform;
use crate::storage::{StorageError, StorageResult};

const LOCK_NAME: &str = ".ironflow-run-leases.lock";
const LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const LOCK_RETRY: std::time::Duration = std::time::Duration::from_millis(10);
type WorkerGate = tokio::sync::Mutex<()>;
static LEASE_WORKER_GATES: LazyLock<Mutex<HashMap<PathBuf, Weak<WorkerGate>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug)]
struct LeaseLock {
    _file: File,
}

impl JsonStateStore {
    pub(super) async fn with_lease_lock<T, F, Fut>(&self, operation: F) -> StorageResult<T>
    where
        T: Send + 'static,
        F: FnOnce(JsonStateStore) -> Fut + Send + 'static,
        Fut: Future<Output = StorageResult<T>> + Send + 'static,
    {
        self.run_leases.ensure_created().await?;
        let canonical_directory = tokio::fs::canonicalize(self.run_leases.path(""))
            .await
            .map_err(|error| {
                StorageError::backend("Failed to resolve JSON run lease directory", error)
            })?;
        #[cfg(test)]
        if let Some(attempted) = self.lease_lock_attempt_hook.lock().unwrap().take() {
            attempted.notify_one();
        }
        let worker_guard = worker_gate(canonical_directory).lock_owned().await;
        let directory = self.run_leases.clone();
        let store = self.clone();
        let runtime = tokio::runtime::Handle::current();
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        std::thread::Builder::new()
            .name("ironflow-json-lease".to_string())
            .spawn(move || {
                // This Tokio-independent thread owns both the process gate and
                // OS lock until the complete target commit settles. Dropping
                // the async caller only drops the receiver; it cannot expose a
                // detached rename to a newer lease owner.
                let _worker_guard = worker_guard;
                let result = acquire_lock(&directory, LOCK_TIMEOUT)
                    .and_then(|_lease_lock| runtime.block_on(operation(store)));
                let _ = result_tx.send(result);
            })
            .map_err(|error| StorageError::backend("Failed to start JSON lease worker", error))?;
        result_rx
            .await
            .map_err(|error| StorageError::backend("JSON run lease worker stopped", error))?
    }
}

fn worker_gate(directory: PathBuf) -> Arc<WorkerGate> {
    let mut gates = LEASE_WORKER_GATES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    gates.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = gates.get(&directory).and_then(Weak::upgrade) {
        return gate;
    }
    let gate = Arc::new(WorkerGate::new(()));
    gates.insert(directory, Arc::downgrade(&gate));
    gate
}

fn acquire_lock(
    directory: &SecureStoreDir,
    timeout: std::time::Duration,
) -> StorageResult<LeaseLock> {
    let lock_path = directory.path(LOCK_NAME);
    let root = lock_path.parent().expect("a lease lock path has a parent");
    let root_metadata = std::fs::symlink_metadata(root).map_err(|error| {
        StorageError::backend("Failed to inspect JSON run lease directory", error)
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(StorageError::corruption(
            "Unsafe JSON run lease directory",
            "lease directory is not a real directory",
        ));
    }
    reject_unsafe_lock_entry(&lock_path)?;

    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    platform::configure_created(&mut options);
    let file = options
        .open(&lock_path)
        .map_err(|error| StorageError::backend("Failed to open JSON run lease lock", error))?;
    if !file
        .metadata()
        .map_err(|error| StorageError::backend("Failed to inspect JSON run lease lock", error))?
        .is_file()
    {
        return Err(StorageError::corruption(
            "Unsafe JSON run lease lock",
            "opened lock entry is not a regular file",
        ));
    }
    platform::harden_created_file(&file)
        .map_err(|error| StorageError::backend("Failed to secure run lease lock", error))?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) => {
                let now = std::time::Instant::now();
                if now >= deadline {
                    return Err(StorageError::backend(
                        "Timed out locking JSON run leases",
                        format_args!("lock remained held for {}ms", timeout.as_millis()),
                    ));
                }
                std::thread::sleep(LOCK_RETRY.min(deadline.saturating_duration_since(now)));
            }
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(StorageError::backend(
                    "Failed to lock JSON run leases",
                    error,
                ));
            }
        }
    }
    Ok(LeaseLock { _file: file })
}

fn reject_unsafe_lock_entry(path: &std::path::Path) -> StorageResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(StorageError::corruption(
                "Unsafe JSON run lease lock",
                "lock entry is not a regular file",
            ))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::backend(
            "Failed to inspect JSON run lease lock",
            error,
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;

    use super::*;

    #[test]
    fn an_external_lease_lock_is_bounded() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = SecureStoreDir::new(temporary.path().join("leases"));
        std::fs::create_dir_all(directory.path("")).unwrap();
        let lock_path = directory.path(LOCK_NAME);
        let external = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .unwrap();
        external.lock().unwrap();

        let started = std::time::Instant::now();
        let error = acquire_lock(&directory, std::time::Duration::from_millis(25)).unwrap_err();

        assert_eq!(error.kind(), crate::storage::StorageErrorKind::Backend);
        assert!(error.to_string().contains("Timed out"));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
