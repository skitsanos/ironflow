#[cfg(target_os = "linux")]
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use super::*;

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn unrelated_non_utf8_entries_are_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let mut bytes = vec![0xff];
    bytes.extend_from_slice(b".json");
    std::fs::write(
        directory.path().join(OsString::from_vec(bytes)),
        b"unmanaged",
    )
    .unwrap();
    let store = JsonStateStore::new(directory.path());

    assert!(store.list_runs(None).await.unwrap().is_empty());
    assert!(store.list_run_summaries(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn base_directory_symlinks_and_non_directories_are_rejected() {
    let outer = tempfile::tempdir().unwrap();
    let target = outer.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let linked = outer.path().join("linked-store");
    symlink(&target, &linked).unwrap();
    let store = JsonStateStore::new(&linked);
    assert_eq!(
        store
            .init_run("safe", "flow", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    assert!(std::fs::read_dir(&target).unwrap().next().is_none());

    let file_path = outer.path().join("file-store");
    std::fs::write(&file_path, b"not a directory").unwrap();
    let store = JsonStateStore::new(&file_path);
    assert_eq!(
        store
            .init_run("safe", "flow", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
}

#[tokio::test]
async fn run_and_orphan_summary_symlinks_are_never_followed() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), b"outside sentinel").unwrap();
    symlink(outside.path(), directory.path().join("victim.json")).unwrap();
    let store = JsonStateStore::new(directory.path());

    assert_eq!(
        store.get_run_info("victim").await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
    assert_eq!(
        store.delete_run("victim").await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
    assert_eq!(
        store.list_runs(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
    assert_eq!(std::fs::read(outside.path()).unwrap(), b"outside sentinel");

    std::fs::remove_file(directory.path().join("victim.json")).unwrap();
    symlink(outside.path(), directory.path().join("orphan.summary.json")).unwrap();
    assert_eq!(
        store.list_runs(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
    assert_eq!(
        store.list_run_summaries(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
    assert_eq!(
        store
            .init_run("orphan", "flow", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    assert_eq!(std::fs::read(outside.path()).unwrap(), b"outside sentinel");
    assert_no_temporary_entries(directory.path());
}

#[tokio::test]
async fn new_and_legacy_entries_are_restricted_to_owner_access() {
    let parent = tempfile::tempdir().unwrap();
    let base = parent.path().join("secure-store");
    let store = JsonStateStore::new(&base);
    store
        .init_run("secure", "flow", &Context::new())
        .await
        .unwrap();

    let run_path = base.join("secure.json");
    let summary_path = base.join("secure.summary.json");
    assert_eq!(mode(&base), 0o700);
    assert_eq!(mode(&run_path), 0o600);
    assert_eq!(mode(&summary_path), 0o600);

    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o777)).unwrap();
    std::fs::set_permissions(&run_path, std::fs::Permissions::from_mode(0o666)).unwrap();
    std::fs::set_permissions(&summary_path, std::fs::Permissions::from_mode(0o666)).unwrap();
    store.get_run_info("secure").await.unwrap();
    store.list_run_summaries(None).await.unwrap();

    assert_eq!(mode(&base), 0o700);
    assert_eq!(mode(&run_path), 0o600);
    assert_eq!(mode(&summary_path), 0o600);
}
