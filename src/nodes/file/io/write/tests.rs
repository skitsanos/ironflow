use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::input::preflight_base64;
use super::*;

#[test]
fn base64_preflight_rejects_amplification_before_decode() {
    let error = preflight_base64(&"A".repeat(100), 4)
        .unwrap_err()
        .to_string();
    assert!(error.contains("IRONFLOW_MAX_FILE_BYTES"), "{error}");
    assert!(preflight_base64("A", 100).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_removes_staging_and_preserves_destination() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("output.bin");
    std::fs::write(&destination, b"original").unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let worker_started = Arc::clone(&started);
    let worker_finished = Arc::clone(&finished);
    let worker_destination = destination.clone();
    let task = tokio::spawn(async move {
        run_tracked_blocking_step(move |execution| {
            let root = RootedDir::prepare(
                worker_destination.parent().unwrap(),
                "write_file",
                &execution,
            )?;
            let mut staged = root.stage_file(
                Path::new(worker_destination.file_name().unwrap()),
                true,
                &execution,
            )?;
            let result = copy_exact(
                SlowReader(worker_started),
                staged.writer(),
                u64::MAX,
                &execution,
                "test input",
            );
            worker_finished.store(true, Ordering::Release);
            result
        })
        .await
    });
    wait_for(&started).await;
    task.abort();
    let _ = task.await;
    wait_for(&finished).await;

    assert_eq!(std::fs::read(&destination).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

struct SlowReader(Arc<AtomicBool>);

impl Read for SlowReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.0.store(true, Ordering::Release);
        std::thread::sleep(Duration::from_millis(5));
        buffer.fill(b'x');
        Ok(buffer.len())
    }
}

async fn wait_for(flag: &AtomicBool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !flag.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
