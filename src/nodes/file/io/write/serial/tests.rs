use std::ffi::OsStr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::nodes::file::RootedDir;
use crate::util::execution::run_blocking_step;

fn key(directory: &Path, leaf: &str, execution: &ExecutionControl) -> Result<DestinationKey> {
    RootedDir::prepare(directory, "test", execution)?.destination_key(OsStr::new(leaf))
}

async fn hold(directory: &Path, leaf: &'static str) -> DestinationGuard {
    let directory = directory.to_path_buf();
    run_blocking_step(move |execution| acquire(key(&directory, leaf, &execution)?, &execution))
        .await
        .unwrap()
}

async fn wait_for(flag: &AtomicBool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !flag.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_waiter_gets_the_destination_only_after_the_holder_releases_it() {
    let directory = tempfile::tempdir().unwrap();
    let holder = hold(directory.path(), "events.log").await;
    let acquired = Arc::new(AtomicBool::new(false));
    let worker_acquired = Arc::clone(&acquired);
    let path = directory.path().to_path_buf();
    let waiter = tokio::spawn(run_blocking_step(move |execution| {
        // The same entry spelled in another case must wait as well.
        let guard = acquire(key(&path, "Events.LOG", &execution)?, &execution)?;
        worker_acquired.store(true, Ordering::Release);
        drop(guard);
        Ok(())
    }));

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !acquired.load(Ordering::Acquire),
        "a second writer entered while the destination was held"
    );
    drop(holder);
    wait_for(&acquired).await;
    waiter.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_waiter_stops_waiting_and_does_not_take_the_destination() {
    let directory = tempfile::tempdir().unwrap();
    let holder = hold(directory.path(), "events.log").await;
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let outcome = Arc::new(Mutex::new(String::new()));
    let (worker_started, worker_finished, worker_outcome) = (
        Arc::clone(&started),
        Arc::clone(&finished),
        Arc::clone(&outcome),
    );
    let path = directory.path().to_path_buf();
    let waiter = tokio::spawn(run_blocking_step(move |execution| {
        let key = key(&path, "events.log", &execution)?;
        worker_started.store(true, Ordering::Release);
        let result = acquire(key, &execution);
        *worker_outcome.lock().unwrap() = match &result {
            Ok(_) => "acquired".to_owned(),
            Err(error) => error.to_string(),
        };
        worker_finished.store(true, Ordering::Release);
        result.map(drop)
    }));

    wait_for(&started).await;
    waiter.abort();
    let _ = waiter.await;
    wait_for(&finished).await;

    assert_eq!(*outcome.lock().unwrap(), "step execution cancelled");
    // The holder still owns the destination and releasing it frees the key.
    drop(holder);
    drop(hold(directory.path(), "events.log").await);
}

#[tokio::test]
async fn other_destinations_are_not_blocked() {
    let directory = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let _held = hold(directory.path(), "events.log").await;

    drop(hold(directory.path(), "other.log").await);
    drop(hold(other.path(), "events.log").await);
}

#[cfg(unix)]
#[tokio::test]
async fn a_directory_alias_names_the_same_destination() {
    let directory = tempfile::tempdir().unwrap();
    let real = directory.path().join("real");
    let alias = directory.path().join("alias");
    std::fs::create_dir(&real).unwrap();
    std::os::unix::fs::symlink(&real, &alias).unwrap();

    run_blocking_step(move |execution| {
        let direct = key(&real, "events.log", &execution)?;
        assert_eq!(direct, key(&alias, "events.log", &execution)?);
        assert_ne!(direct, key(&real, "other.log", &execution)?);
        Ok(())
    })
    .await
    .unwrap();
}
