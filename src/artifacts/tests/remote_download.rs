mod server;

use std::io::Read;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::util::execution::{
    CooperativeWorkerSet, run_tracked_blocking_step, with_attempt_worker_set,
    with_execution_deadline, with_run_worker_set,
};
use server::{CHUNKS, Server};

#[derive(Default)]
struct ExitGate(Mutex<bool>, Condvar);

impl ExitGate {
    fn hold(&self) {
        let guard = self.0.lock().unwrap();
        drop(self.1.wait_while(guard, |released| !*released).unwrap());
    }
    fn release(&self) {
        *self.0.lock().unwrap() = true;
        self.1.notify_all();
    }
}

struct ReleaseOnDrop(Arc<ExitGate>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

async fn wait_for_staging(root: &std::path::Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let started = std::fs::read_dir(root.join("sha256"))
                .unwrap()
                .any(|entry| {
                    let entry = entry.unwrap();
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".ironflow-artifact-")
                        && entry.metadata().is_ok_and(|meta| meta.len() > 0)
                });
            if started {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("download never staged a chunk");
}

#[tokio::test]
async fn progressing_download_stops_before_eof_and_retains_worker_tracking() {
    for uri in [false, true] {
        for deadline in [false, true] {
            check_cancel(uri, deadline, false).await;
        }
    }
}

#[tokio::test]
async fn idle_download_still_observes_cancellation_and_deadlines() {
    for deadline in [false, true] {
        check_cancel(false, deadline, true).await;
    }
}

#[tokio::test]
async fn complete_downloads_verify_bytes_and_reuse_the_cache() {
    for uri in [false, true] {
        let server = Server::start(Duration::from_millis(1), false, false).await;
        let directory = tempfile::tempdir().unwrap();
        let store = server.store(directory.path());
        for _ in 0..2 {
            let reader_store = store.clone();
            let artifact = server.artifact.clone();
            let restored = run_tracked_blocking_step(move |execution| {
                let mut file = if uri {
                    reader_store.open_uri(&artifact.artifact_uri, &execution)?
                } else {
                    reader_store.open(&artifact, &execution)?
                };
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)?;
                Ok(bytes)
            })
            .await
            .unwrap();
            assert_eq!(restored, server.bytes);
        }
        assert_eq!(
            server.requests.load(Ordering::Acquire),
            1,
            "cache hit fetched again"
        );
        assert_eq!(server.sent.load(Ordering::Acquire), CHUNKS);
        let path = store.resolve(&server.artifact).unwrap();
        assert!(std::fs::metadata(path).unwrap().permissions().readonly());
        assert_eq!(
            std::fs::read_dir(directory.path().join("sha256"))
                .unwrap()
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn corrupt_downloads_fail_integrity_and_remove_staging() {
    for uri in [false, true] {
        let server = Server::start(Duration::from_millis(1), true, false).await;
        let directory = tempfile::tempdir().unwrap();
        let store = server.store(directory.path());
        let artifact = server.artifact.clone();
        let error = run_tracked_blocking_step(move |execution| {
            if uri {
                store.open_uri(&artifact.artifact_uri, &execution)
            } else {
                store.open(&artifact, &execution)
            }
        })
        .await
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("digest verification"),
            "{error:#}"
        );
        super::assert_digest_directory_empty(directory.path());
    }
}

async fn check_cancel(uri: bool, deadline: bool, idle: bool) {
    let server = Server::start(Duration::from_millis(20), false, idle).await;
    let directory = tempfile::tempdir().unwrap();
    let store = server.store(directory.path());
    let artifact = server.artifact.clone();
    let run_workers = CooperativeWorkerSet::new();
    let attempt_workers = CooperativeWorkerSet::new();
    let run = run_workers.clone();
    let attempt = attempt_workers.clone();
    let gate = Arc::new(ExitGate::default());
    let release = ReleaseOnDrop(gate.clone());
    let (returned, receive) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        with_execution_deadline(
            deadline.then(|| tokio::time::Instant::now() + Duration::from_secs(1)),
            with_run_worker_set(
                run,
                with_attempt_worker_set(
                    attempt,
                    run_tracked_blocking_step(move |execution| {
                        let result = if uri {
                            store.open_uri(&artifact.artifact_uri, &execution)
                        } else {
                            store.open(&artifact, &execution)
                        };
                        let _ =
                            returned.send(result.map(|_| ()).map_err(|error| format!("{error:#}")));
                        gate.hold();
                        execution.checkpoint()
                    }),
                ),
            ),
        )
        .await
    });
    wait_for_staging(directory.path()).await;
    if !deadline {
        task.abort();
    }
    let result =
        tokio::time::timeout(Duration::from_secs(if deadline { 2 } else { 1 }), receive).await;
    assert!(
        result.is_ok(),
        "cancelled restore kept downloading: uri={uri}, deadline={deadline}, chunks={}",
        server.sent.load(Ordering::Acquire)
    );
    let error = result.unwrap().unwrap().unwrap_err();
    assert!(
        error.contains(if deadline {
            "deadline exceeded"
        } else {
            "cancelled"
        }),
        "{error}"
    );
    assert!(
        server.sent.load(Ordering::Acquire) < CHUNKS,
        "download drained to EOF"
    );
    super::assert_digest_directory_empty(directory.path());
    for workers in [&run_workers, &attempt_workers] {
        assert!(
            tokio::time::timeout(Duration::from_millis(25), workers.wait_until_idle())
                .await
                .is_err(),
            "worker tracking ended before physical exit"
        );
    }
    release.0.release();
    tokio::time::timeout(Duration::from_secs(2), async {
        run_workers.wait_until_idle().await;
        attempt_workers.wait_until_idle().await;
    })
    .await
    .expect("physical worker did not drain");
    let _ = task.await;
    super::assert_digest_directory_empty(directory.path());
}
