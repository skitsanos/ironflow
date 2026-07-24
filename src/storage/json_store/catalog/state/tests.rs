use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use super::super::{CATALOG_NAME, STATE_NAME};
use super::{current_token, mark_clean, mark_dirty};
use crate::storage::json_store::JsonStateStore;
use crate::storage::json_store::catalog::delta::{self, DELTA_NAME};
use crate::storage::json_store::catalog::format;

async fn write_base_and_delta(store: &JsonStateStore) -> (Uuid, Uuid) {
    store.directory.ensure_created().await.unwrap();
    mark_dirty(&store.directory).await.unwrap();
    let (base_generation, base) = format::encode(&mut []).unwrap();
    store
        .directory
        .write_replace(CATALOG_NAME, &base)
        .await
        .unwrap();
    let (delta_revision, delta) = delta::encode(base_generation, []).unwrap();
    store
        .directory
        .write_replace(DELTA_NAME, &delta)
        .await
        .unwrap();
    (base_generation, delta_revision)
}

#[tokio::test]
async fn clean_token_binds_base_and_delta_generations() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    let (base_generation, delta_revision) = write_base_and_delta(&store).await;
    let token = mark_clean(&store.directory, base_generation, delta_revision)
        .await
        .unwrap();

    let current = current_token(&store.directory).await.unwrap().unwrap();
    assert_eq!(current, token);
    assert_eq!(current.base_generation(), base_generation);
    assert_eq!(current.delta_revision(), delta_revision);

    let (_, replacement) = delta::encode(base_generation, []).unwrap();
    store
        .directory
        .write_replace(DELTA_NAME, &replacement)
        .await
        .unwrap();
    assert!(current_token(&store.directory).await.unwrap().is_none());
}

#[tokio::test]
async fn clean_token_rejects_mismatched_or_corrupt_delta() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    let (base_generation, delta_revision) = write_base_and_delta(&store).await;
    mark_clean(&store.directory, base_generation, delta_revision)
        .await
        .unwrap();

    let (_, mismatched) = delta::encode(Uuid::new_v4(), []).unwrap();
    store
        .directory
        .write_replace(DELTA_NAME, &mismatched)
        .await
        .unwrap();
    let replacement_revision = delta::decode(&mismatched).unwrap().revision;
    mark_clean(&store.directory, base_generation, replacement_revision)
        .await
        .unwrap();
    assert!(current_token(&store.directory).await.unwrap().is_none());

    store
        .directory
        .write_replace(DELTA_NAME, b"truncated")
        .await
        .unwrap();
    mark_clean(&store.directory, base_generation, replacement_revision)
        .await
        .unwrap();
    assert!(current_token(&store.directory).await.unwrap().is_none());
}

#[tokio::test]
async fn version_one_clean_state_is_not_accepted() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    let (base_generation, _) = write_base_and_delta(&store).await;
    let mut legacy = Vec::with_capacity(104);
    legacy.extend_from_slice(b"IFLOWCATSTATEV1!");
    legacy.extend_from_slice(&1_u32.to_be_bytes());
    legacy.push(1);
    legacy.extend_from_slice(&[0; 3]);
    legacy.extend_from_slice(base_generation.as_bytes());
    legacy.resize(72, 0);
    let checksum = Sha256::digest(&legacy);
    legacy.extend_from_slice(&checksum);
    store
        .directory
        .write_replace(STATE_NAME, &legacy)
        .await
        .unwrap();

    assert!(current_token(&store.directory).await.unwrap().is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn delta_symlink_is_rejected_without_following_it() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), b"outside sentinel").unwrap();
    let store = JsonStateStore::new(directory.path());
    let (base_generation, delta_revision) = write_base_and_delta(&store).await;
    mark_clean(&store.directory, base_generation, delta_revision)
        .await
        .unwrap();
    std::fs::remove_file(directory.path().join(DELTA_NAME)).unwrap();
    symlink(outside.path(), directory.path().join(DELTA_NAME)).unwrap();

    assert!(current_token(&store.directory).await.is_err());
    assert_eq!(std::fs::read(outside.path()).unwrap(), b"outside sentinel");
}
