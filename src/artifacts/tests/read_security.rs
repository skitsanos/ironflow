use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{SlowReader, store_bytes, wait_for_flag};
use crate::artifacts::LocalArtifactStore;
use crate::util::execution::run_blocking_step;

#[tokio::test]
async fn same_size_replacement_fails_read_verification() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    let artifact = store_bytes(store.clone(), b"trusted".to_vec(), 100)
        .await
        .unwrap();
    let path = store.resolve(&artifact).unwrap();
    make_writable(&path);
    std::fs::write(&path, b"hostile").unwrap();

    let error = run_blocking_step(move |execution| store.open(&artifact, &execution))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("digest verification"), "{error}");
}

#[cfg(unix)]
#[tokio::test]
async fn verified_open_keeps_the_hashed_inode_after_path_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    let artifact = store_bytes(store.clone(), b"trusted".to_vec(), 100)
        .await
        .unwrap();
    let path = store.resolve(&artifact).unwrap();
    let moved = directory.path().join("moved-artifact");
    let open_store = store.clone();
    let open_artifact = artifact.clone();
    let mut verified =
        run_blocking_step(move |execution| open_store.open(&open_artifact, &execution))
            .await
            .unwrap();

    std::fs::rename(&path, &moved).unwrap();
    std::fs::write(&path, b"hostile").unwrap();
    let mut bytes = Vec::new();
    verified.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"trusted");
}

#[tokio::test]
async fn verified_path_lease_is_read_only_private_and_removed_on_drop() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    let artifact = store_bytes(store.clone(), b"leased".to_vec(), 100)
        .await
        .unwrap();
    let lease_store = store.clone();
    let lease = run_blocking_step(move |execution| {
        lease_store.verified_path_lease(&artifact, 100, &execution)
    })
    .await
    .unwrap();
    let path = lease.path().to_owned();
    assert_eq!(std::fs::read(&path).unwrap(), b"leased");
    assert!(std::fs::metadata(&path).unwrap().permissions().readonly());
    assert!(
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".ironflow-artifact-"))
    );
    drop(lease);
    assert!(!path.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn verified_open_rejects_a_symlink_at_a_valid_digest_path() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path().join("artifacts")).unwrap();
    let outside = directory.path().join("outside");
    std::fs::write(&outside, b"outside").unwrap();
    let digest = "a".repeat(64);
    symlink(&outside, store.root().join("sha256").join(&digest)).unwrap();

    let uri = format!("artifact://sha256/{digest}");
    let error = run_blocking_step(move |execution| store.open_uri(&uri, &execution))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("failed to open file"), "{error}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_verification_observes_cancellation() {
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let worker_started = Arc::clone(&started);
    let worker_finished = Arc::clone(&finished);
    let task = tokio::spawn(async move {
        run_blocking_step(move |execution| {
            let result = super::super::integrity::hash_reader(
                &mut SlowReader(worker_started),
                u64::MAX,
                &execution,
            );
            worker_finished.store(true, Ordering::Release);
            result
        })
        .await
    });
    wait_for_flag(&started).await;
    task.abort();
    let _ = task.await;
    wait_for_flag(&finished).await;
}

#[cfg(unix)]
fn make_writable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn make_writable(path: &std::path::Path) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(false);
    std::fs::set_permissions(path, permissions).unwrap();
}
