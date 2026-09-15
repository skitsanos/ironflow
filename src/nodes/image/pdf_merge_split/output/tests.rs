use super::*;
use crate::util::execution::{
    CooperativeWorkerSet, run_tracked_blocking_step, with_run_worker_set,
};

fn policy(maximum: u64) -> Policy {
    Policy {
        operation: "pdf_split",
        variable: "IRONFLOW_MAX_PDF_BYTES",
        maximum,
        overwrite: false,
    }
}

#[tokio::test]
async fn output_limit_removes_staging_without_publishing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_owned();
    let error = run_tracked_blocking_step(move |execution| {
        let root = RootedDir::prepare(&path, "pdf_split", &execution)?;
        save_atomic(
            &mut Document::new(),
            &root,
            Path::new("part.pdf"),
            policy(1),
            &execution,
        )
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("IRONFLOW_MAX_PDF_BYTES"), "{error}");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn cancellation_during_staged_writes_cleans_up_before_worker_releases() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_owned();
    let workers = CooperativeWorkerSet::new();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (observed_tx, observed_rx) = tokio::sync::oneshot::channel();
    let (continue_tx, continue_rx) = std::sync::mpsc::channel();
    let task = tokio::spawn(with_run_worker_set(
        workers.clone(),
        run_tracked_blocking_step(move |execution| {
            let root = RootedDir::prepare(&path, "pdf_split", &execution)?;
            let mut staged = root.stage_file(Path::new("part.pdf"), false, &execution)?;
            let policy = policy(1024);
            let mut writer = CappedWriter {
                inner: staged.writer(),
                policy: &policy,
                written: 0,
                execution: &execution,
            };
            writer.write_all(b"partial PDF")?;
            ready_tx.send(()).unwrap();
            continue_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            observed_tx
                .send(
                    writer
                        .write_all(b"must not be written")
                        .map_err(|error| error.to_string()),
                )
                .unwrap();
            Ok(())
        }),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(5), ready_rx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    continue_tx.send(()).unwrap();
    let error = tokio::time::timeout(std::time::Duration::from_secs(5), observed_rx)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.contains("cancelled"), "{error}");
    tokio::time::timeout(std::time::Duration::from_secs(5), workers.wait_until_idle())
        .await
        .unwrap();
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn no_overwrite_commit_rejects_destination_created_after_staging() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_owned();
    let error = run_tracked_blocking_step(move |execution| {
        let root = RootedDir::prepare(&path, "pdf_split", &execution)?;
        let mut staged = root.stage_file(Path::new("part.pdf"), false, &execution)?;
        staged.writer().write_all(b"new")?;
        std::fs::write(path.join("part.pdf"), b"existing")?;
        staged.commit()
    })
    .await
    .unwrap_err();
    assert!(!error.to_string().is_empty());
    assert_eq!(
        std::fs::read(directory.path().join("part.pdf")).unwrap(),
        b"existing"
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
