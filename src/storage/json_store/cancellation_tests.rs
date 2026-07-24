use std::sync::{Arc, Barrier, mpsc};
use std::time::Duration;

use super::temp::{TempFile, spawn_guarded_open};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_temporary_open_cleans_the_created_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(".cancelled.json.test.tmp");
    let worker_path = path.clone();
    let (created_tx, created_rx) = mpsc::sync_channel(1);
    let release = Arc::new(Barrier::new(2));
    let worker_release = release.clone();

    let waiter = tokio::spawn(async move {
        spawn_guarded_open(move || {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&worker_path)?;
            let temporary = TempFile::new(worker_path);
            created_tx.send(()).unwrap();
            worker_release.wait();
            Ok((temporary, file))
        })
        .await
    });

    tokio::task::spawn_blocking(move || created_rx.recv().unwrap())
        .await
        .unwrap();
    assert!(path.exists());
    waiter.abort();
    let _ = waiter.await;
    tokio::task::spawn_blocking(move || release.wait())
        .await
        .unwrap();

    for _ in 0..100 {
        if !path.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("cancelled guarded open left a temporary file behind");
}
