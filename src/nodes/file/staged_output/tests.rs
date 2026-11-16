use std::path::PathBuf;
use std::time::Duration;

use super::*;
use crate::util::execution::{
    CooperativeWorkerSet, run_tracked_blocking_step, with_run_worker_set,
};

const ORIGINAL: &[u8] = b"previous output";
const WAIT: Duration = Duration::from_secs(5);

fn existing_destination() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("output.bin");
    std::fs::write(&destination, ORIGINAL).unwrap();
    (directory, destination)
}

fn entries(directory: &tempfile::TempDir) -> usize {
    std::fs::read_dir(directory.path()).unwrap().count()
}

#[tokio::test]
async fn publishes_exactly_the_encoded_bytes_over_a_regular_destination() {
    let (directory, destination) = existing_destination();
    // Larger than both the buffer and one checkpointed chunk, in uneven writes.
    let payload: Vec<u8> = (0..300_000_u32).map(|index| (index % 251) as u8).collect();
    let expected = payload.clone();
    let path = destination.clone();

    run_tracked_blocking_step(move |execution| {
        publish(&path, "test_node", &execution, |writer| {
            let (head, tail) = payload.split_at(7);
            writer.write_all(head)?;
            writer.write_all(tail)?;
            Ok(())
        })
    })
    .await
    .unwrap();

    assert_eq!(std::fs::read(&destination).unwrap(), expected);
    assert_eq!(entries(&directory), 1);
}

#[tokio::test]
async fn a_failed_encode_keeps_the_destination_and_removes_the_staged_file() {
    let (directory, destination) = existing_destination();
    let path = destination.clone();

    let error = run_tracked_blocking_step(move |execution| {
        publish(&path, "test_node", &execution, |writer| {
            writer.write_all(b"partial output")?;
            writer.flush()?;
            anyhow::bail!("encoder refused the pixels")
        })
    })
    .await
    .unwrap_err();

    assert_eq!(error.to_string(), "encoder refused the pixels");
    assert_eq!(std::fs::read(&destination).unwrap(), ORIGINAL);
    assert_eq!(entries(&directory), 1);
}

#[tokio::test]
async fn cancellation_mid_encode_cleans_up_before_the_worker_is_released() {
    let (directory, destination) = existing_destination();
    let path = destination.clone();
    let workers = CooperativeWorkerSet::new();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (observed_tx, observed_rx) = tokio::sync::oneshot::channel();
    let (continue_tx, continue_rx) = std::sync::mpsc::channel();
    let (refused_tx, refused_rx) = std::sync::mpsc::channel();
    let task = tokio::spawn(with_run_worker_set(
        workers.clone(),
        run_tracked_blocking_step(move |execution| {
            let published = publish(&path, "test_node", &execution, |writer| {
                writer.write_all(b"partial output")?;
                writer.flush()?;
                ready_tx.send(()).unwrap();
                continue_rx.recv_timeout(WAIT).unwrap();
                let late = writer.write_all(&[0_u8; 128 * 1024]);
                refused_tx.send(late.is_err()).unwrap();
                late?;
                Ok(())
            });
            observed_tx
                .send(published.map_err(|error| format!("{error:#}")))
                .unwrap();
            Ok(())
        }),
    ));
    tokio::time::timeout(WAIT, ready_rx).await.unwrap().unwrap();
    assert_eq!(entries(&directory), 2, "the staged file sits beside it");

    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    continue_tx.send(()).unwrap();

    let error = tokio::time::timeout(WAIT, observed_rx)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error, "step execution cancelled");
    assert!(
        refused_rx.recv_timeout(WAIT).unwrap(),
        "the write after cancellation must be refused, not only the commit"
    );
    tokio::time::timeout(WAIT, workers.wait_until_idle())
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), ORIGINAL);
    assert_eq!(entries(&directory), 1);
}

#[tokio::test]
async fn cancellation_after_the_last_write_does_not_publish() {
    let (directory, destination) = existing_destination();
    let path = destination.clone();
    let workers = CooperativeWorkerSet::new();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (observed_tx, observed_rx) = tokio::sync::oneshot::channel();
    let (continue_tx, continue_rx) = std::sync::mpsc::channel();
    let task = tokio::spawn(with_run_worker_set(
        workers.clone(),
        run_tracked_blocking_step(move |execution| {
            let published = publish(&path, "test_node", &execution, |writer| {
                writer.write_all(b"complete output")?;
                writer.flush()?;
                ready_tx.send(()).unwrap();
                continue_rx.recv_timeout(WAIT).unwrap();
                Ok(())
            });
            observed_tx
                .send(published.map_err(|error| format!("{error:#}")))
                .unwrap();
            Ok(())
        }),
    ));
    tokio::time::timeout(WAIT, ready_rx).await.unwrap().unwrap();

    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    continue_tx.send(()).unwrap();

    let error = tokio::time::timeout(WAIT, observed_rx)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error, "step execution cancelled");
    tokio::time::timeout(WAIT, workers.wait_until_idle())
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), ORIGINAL);
    assert_eq!(entries(&directory), 1);
}

#[tokio::test]
async fn a_missing_directory_is_an_error_and_is_not_created() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing/deeper/output.bin");

    let error = run_tracked_blocking_step(move |execution| {
        publish(&path, "test_node", &execution, |writer| {
            writer.write_all(b"never written")?;
            Ok(())
        })
    })
    .await
    .unwrap_err()
    .to_string();

    assert!(
        error.starts_with("test_node: destination directory"),
        "{error}"
    );
    assert!(error.ends_with("does not exist"), "{error}");
    assert_eq!(entries(&directory), 0);
}

#[tokio::test]
async fn a_destination_without_a_file_name_is_refused() {
    let error = run_tracked_blocking_step(move |execution| {
        publish(Path::new("/"), "test_node", &execution, |_writer| Ok(()))
    })
    .await
    .unwrap_err()
    .to_string();

    assert_eq!(error, "test_node: output path has no file name");
}
