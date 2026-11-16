//! The portable writer is compiled into Unix test builds so its contract is
//! exercised without a Windows host (IF-218 e).

use std::ffi::OsStr;
use std::io::{Read, Write};

use super::*;

async fn run<T: Send + 'static>(
    work: impl FnOnce(ExecutionControl) -> Result<T> + Send + 'static,
) -> Result<T> {
    crate::util::execution::run_tracked_blocking_step(work).await
}

fn staging_leftovers(directory: &Path) -> usize {
    fs::read_dir(directory)
        .unwrap()
        .filter(|entry| {
            let name = entry.as_ref().unwrap().file_name();
            name.to_string_lossy().starts_with(".ironflow-")
        })
        .count()
}

#[tokio::test]
async fn existing_refuses_a_missing_directory_and_creates_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("absent").join("deeper");
    let target = missing.clone();

    let error = run(move |execution| RootedDir::existing(&target, "test", &execution).map(|_| ()))
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("does not exist"), "{error}");
    assert!(!directory.path().join("absent").exists());
}

#[tokio::test]
async fn existing_stages_and_replaces_a_regular_file() {
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().to_path_buf();
    fs::write(root_path.join("out.txt"), b"before").unwrap();

    let target = root_path.clone();
    run(move |execution| {
        let root = RootedDir::existing(&target, "test", &execution)?;
        let mut staged = root.stage_file(Path::new("out.txt"), true, &execution)?;
        staged.writer().write_all(b"after")?;
        // Nothing is visible at the destination until the commit.
        assert_eq!(fs::read(target.join("out.txt"))?, b"before");
        staged.commit()
    })
    .await
    .unwrap();

    assert_eq!(fs::read(root_path.join("out.txt")).unwrap(), b"after");
    assert_eq!(staging_leftovers(&root_path), 0);
}

#[tokio::test]
async fn prepare_creates_missing_directories_and_nested_parents() {
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().join("made").join("deeper");

    let target = root_path.clone();
    run(move |execution| {
        let root = RootedDir::prepare(&target, "test", &execution)?;
        root.ensure_dir(Path::new("empty"), &execution)?;
        let mut staged = root.stage_file(Path::new("nested/new.txt"), false, &execution)?;
        staged.writer().write_all(b"new")?;
        staged.commit()
    })
    .await
    .unwrap();

    assert!(root_path.join("empty").is_dir());
    assert_eq!(fs::read(root_path.join("nested/new.txt")).unwrap(), b"new");
    assert_eq!(staging_leftovers(&root_path.join("nested")), 0);
}

#[tokio::test]
async fn without_overwrite_an_existing_file_is_kept_and_staging_is_removed() {
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().to_path_buf();

    let target = root_path.clone();
    let error = run(move |execution| {
        let root = RootedDir::existing(&target, "test", &execution)?;
        let mut staged = root.stage_file(Path::new("out.txt"), false, &execution)?;
        staged.writer().write_all(b"late")?;
        // The destination appears between staging and publication.
        fs::write(target.join("out.txt"), b"first")?;
        staged.commit()
    })
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("already exists"), "{error}");
    assert_eq!(fs::read(root_path.join("out.txt")).unwrap(), b"first");
    assert_eq!(staging_leftovers(&root_path), 0);
}

#[tokio::test]
async fn a_dropped_staged_file_leaves_no_debris() {
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().to_path_buf();

    let target = root_path.clone();
    run(move |execution| {
        let root = RootedDir::existing(&target, "test", &execution)?;
        let mut staged = root.stage_file(Path::new("out.txt"), true, &execution)?;
        staged.writer().write_all(b"abandoned")?;
        assert_eq!(staging_leftovers(&target), 1);
        Ok(())
    })
    .await
    .unwrap();

    assert_eq!(fs::read_dir(&root_path).unwrap().count(), 0);
}

#[tokio::test]
async fn open_existing_rename_into_and_keys_follow_the_shared_contract() {
    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().join("root");
    fs::create_dir(&root_path).unwrap();
    fs::write(root_path.join("log.txt"), b"line").unwrap();
    let source = directory.path().join("incoming.txt");
    fs::write(&source, b"moved").unwrap();

    let target = root_path.clone();
    run(move |execution| {
        let root = RootedDir::existing(&target, "test", &execution)?;
        assert!(
            root.open_existing(OsStr::new("absent.txt"), &execution)?
                .is_none()
        );
        let mut contents = String::new();
        root.open_existing(OsStr::new("log.txt"), &execution)?
            .unwrap()
            .read_to_string(&mut contents)?;
        assert_eq!(contents, "line");
        assert_eq!(
            root.destination_key(OsStr::new("Log.TXT"))?,
            root.destination_key(OsStr::new("log.txt"))?
        );
        root.rename_into(&source, OsStr::new("log.txt"), &execution)
    })
    .await
    .unwrap();

    assert_eq!(fs::read(root_path.join("log.txt")).unwrap(), b"moved");
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_leaves_and_components_are_refused() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let root_path = directory.path().join("root");
    let outside = directory.path().join("outside");
    fs::create_dir(&root_path).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("victim.txt"), b"keep me").unwrap();
    symlink(outside.join("victim.txt"), root_path.join("leaf.txt")).unwrap();
    symlink(&outside, root_path.join("linked")).unwrap();

    let target = root_path.clone();
    run(move |execution| {
        let root = RootedDir::existing(&target, "test", &execution)?;
        for overwrite in [true, false] {
            let error = root
                .stage_file(Path::new("leaf.txt"), overwrite, &execution)
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default();
            assert!(error.contains("symlink"), "{error}");
        }
        let error = root
            .stage_file(Path::new("linked/new.txt"), true, &execution)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(error.contains("is a symlink"), "{error}");
        let error = root
            .open_existing(OsStr::new("leaf.txt"), &execution)
            .err()
            .map(|error| format!("{error:#}"))
            .unwrap_or_default();
        assert!(
            !error.is_empty(),
            "a symlinked append source must be refused"
        );
        Ok(())
    })
    .await
    .unwrap();

    assert_eq!(fs::read(outside.join("victim.txt")).unwrap(), b"keep me");
    assert!(!outside.join("new.txt").exists());
}
