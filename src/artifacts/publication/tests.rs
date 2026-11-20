use std::io::Write;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::*;
use crate::util::execution::run_tracked_blocking_step;

const BYTES: &[u8] = b"artifact";
const SIZE: u64 = BYTES.len() as u64;

fn staged(dir: &Path) -> TempArtifact {
    let path = dir.join("staged");
    let mut file = filesystem::create_private_file(&path).unwrap();
    file.write_all(BYTES).unwrap();
    filesystem::harden_staging_file(&file).unwrap();
    TempArtifact::new(path, file)
}

async fn cancelled_control() -> ExecutionControl {
    let (send, control) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let owner = tokio::spawn(run_tracked_blocking_step(move |execution| {
        send.send(execution).unwrap();
        wait.recv().unwrap();
        Ok(())
    }));
    let control = control.await.unwrap();
    owner.abort();
    assert!(owner.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    control
}

#[tokio::test]
async fn cancellation_in_duplicate_verification_preserves_the_published_file() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("published");
    let digest = hex::encode(Sha256::digest(BYTES));
    let control = run_tracked_blocking_step(Ok).await.unwrap();
    publish(
        staged(dir.path()),
        &dest,
        &digest,
        SIZE,
        dir.path(),
        &control,
    )
    .unwrap();
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&dest.metadata().unwrap());

    // Start at the collision branch, after publish's entry checkpoint. The
    // first hash checkpoint now observes real owner cancellation without a race.
    let error = reuse_or_repair(
        staged(dir.path()),
        &dest,
        &digest,
        SIZE,
        dir.path(),
        &cancelled_control().await,
    )
    .unwrap_err();
    assert!(error.to_string().contains("cancelled"), "{error:#}");
    assert_eq!(std::fs::read(&dest).unwrap(), BYTES);
    #[cfg(unix)]
    assert_eq!(
        std::os::unix::fs::MetadataExt::ino(&dest.metadata().unwrap()),
        inode
    );
    assert!(!dir.path().join("staged").exists());
}

#[tokio::test]
async fn failed_repair_keeps_the_previous_cache_entry() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("published");
    std::fs::write(&dest, b"damaged!").unwrap();
    filesystem::harden_published_path(&dest).unwrap();
    let temporary = staged(dir.path());
    std::fs::rename(temporary.path(), dir.path().join("moved")).unwrap();
    let control = run_tracked_blocking_step(Ok).await.unwrap();
    reuse_or_repair(
        temporary,
        &dest,
        &hex::encode(Sha256::digest(BYTES)),
        SIZE,
        dir.path(),
        &control,
    )
    .unwrap_err();
    assert_eq!(std::fs::read(&dest).unwrap(), b"damaged!");
    assert!(dest.metadata().unwrap().permissions().readonly());
}

#[tokio::test]
async fn publication_repairs_both_kinds_of_confirmed_mismatch() {
    let control = run_tracked_blocking_step(Ok).await.unwrap();
    let digest = hex::encode(Sha256::digest(BYTES));
    for damaged in [b"x".as_slice(), b"damaged!"] {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("published");
        std::fs::write(&dest, damaged).unwrap();
        filesystem::harden_published_path(&dest).unwrap();
        publish(
            staged(dir.path()),
            &dest,
            &digest,
            SIZE,
            dir.path(),
            &control,
        )
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), BYTES);
        assert!(dest.metadata().unwrap().permissions().readonly());
        assert!(!dir.path().join("staged").exists());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn unreadable_regular_file_is_not_treated_as_corrupt() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("published");
    std::fs::write(&dest, BYTES).unwrap();
    let inode = dest.metadata().unwrap().ino();
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(&dest).is_ok() {
        return; // A privileged test process cannot exercise this permission failure.
    }
    let control = run_tracked_blocking_step(Ok).await.unwrap();
    let digest = hex::encode(Sha256::digest(BYTES));
    publish(
        staged(dir.path()),
        &dest,
        &digest,
        SIZE,
        dir.path(),
        &control,
    )
    .unwrap_err();
    let metadata = dest.metadata().unwrap();
    assert_eq!(metadata.ino(), inode);
    assert_eq!(metadata.permissions().mode() & 0o777, 0);
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o400)).unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), BYTES);
    assert!(!dir.path().join("staged").exists());
}
