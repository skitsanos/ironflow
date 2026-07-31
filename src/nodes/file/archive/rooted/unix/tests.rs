use std::io::{Read, Write};
use std::os::unix::fs::symlink;

use super::*;
use crate::nodes::file::archive::copy::copy_with_control;

struct EndlessReader {
    started: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Read for EndlessReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if let Some(started) = self.started.take() {
            let _ = started.send(());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        buffer.fill(b'x');
        Ok(buffer.len())
    }
}

#[tokio::test]
async fn pinned_root_does_not_follow_a_replacement_symlink() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("destination");
    let pinned = directory.path().join("pinned");
    let outside = directory.path().join("outside");
    std::fs::create_dir(&destination).unwrap();
    std::fs::create_dir(&outside).unwrap();

    let destination_for_worker = destination.clone();
    let pinned_for_worker = pinned.clone();
    let outside_for_worker = outside.clone();
    crate::util::execution::run_blocking_step(move |execution| {
        let root = RootedDir::prepare(&destination_for_worker, "test", &execution)?;
        std::fs::rename(&destination_for_worker, &pinned_for_worker)?;
        symlink(&outside_for_worker, &destination_for_worker)?;

        let mut staged = root.stage_file(Path::new("safe.txt"), true, &execution)?;
        staged.writer().write_all(b"safe")?;
        staged.commit()
    })
    .await
    .unwrap();

    assert_eq!(std::fs::read(pinned.join("safe.txt")).unwrap(), b"safe");
    assert!(!outside.join("safe.txt").exists());
}

#[test]
fn cancelled_copy_cleans_temp_before_blocking_capacity_is_reused() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().to_path_buf();
        let final_path = destination.join("target.txt");
        std::fs::write(&final_path, b"original").unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let worker_destination = destination.clone();

        let waiter = tokio::spawn(crate::util::execution::run_tracked_blocking_step(
            move |execution| {
                let root = RootedDir::prepare(&worker_destination, "test", &execution)?;
                let mut staged = root.stage_file(Path::new("target.txt"), true, &execution)?;
                copy_with_control(
                    &mut EndlessReader {
                        started: Some(started_tx),
                    },
                    staged.writer(),
                    &execution,
                    u64::MAX,
                    "test",
                )?;
                staged.commit()
            },
        ));

        started_rx.await.unwrap();
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());

        let sentinel = tokio::task::spawn_blocking(move || {
            let contents = std::fs::read(&final_path).unwrap();
            let temporary_count = std::fs::read_dir(&destination)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".ironflow-")
                })
                .count();
            (contents, temporary_count)
        });
        let (contents, temporary_count) =
            tokio::time::timeout(std::time::Duration::from_secs(1), sentinel)
                .await
                .expect("cancelled ZIP copy retained the only blocking slot")
                .unwrap();
        assert_eq!(contents, b"original");
        assert_eq!(temporary_count, 0);
    });
}
