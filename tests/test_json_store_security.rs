use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ironflow::engine::types::{Context, RunStatus, TaskState};
use ironflow::storage::json_store::JsonStateStore;
use ironflow::storage::{StateStore, StorageErrorKind};

fn assert_no_temporary_entries(directory: &Path) {
    let names = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        names.iter().all(|name| !name.ends_with(".tmp")),
        "temporary JSON files were not cleaned: {names:?}"
    );
}

#[tokio::test]
async fn invalid_ids_are_rejected_before_any_filesystem_access() {
    let parent = tempfile::tempdir().unwrap();
    let base = parent.path().join("store");
    let store = JsonStateStore::new(&base);
    let invalid = "../outside";
    let task = TaskState::new("step", "log");

    assert_eq!(
        store
            .init_run(invalid, "flow", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::InvalidInput
    );
    assert_eq!(
        store
            .set_run_status(invalid, RunStatus::Running)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::InvalidInput
    );
    assert_eq!(
        store.upsert_task(invalid, &task).await.unwrap_err().kind(),
        StorageErrorKind::InvalidInput
    );
    assert_eq!(
        store.get_ctx(invalid).await.unwrap_err().kind(),
        StorageErrorKind::InvalidInput
    );
    assert_eq!(
        store
            .update_ctx(invalid, &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::InvalidInput
    );
    assert_eq!(
        store.get_run_info(invalid).await.unwrap_err().kind(),
        StorageErrorKind::InvalidInput
    );
    assert_eq!(
        store.delete_run(invalid).await.unwrap_err().kind(),
        StorageErrorKind::InvalidInput
    );

    for invalid in [
        "",
        "-leading",
        "trailing_",
        "contains.dot",
        "contains space",
        "nonascii-é",
        &"a".repeat(129),
    ] {
        assert_eq!(
            store
                .init_run(invalid, "flow", &Context::new())
                .await
                .unwrap_err()
                .kind(),
            StorageErrorKind::InvalidInput
        );
    }
    assert!(!base.exists());
    assert!(!parent.path().join("outside.json").exists());
}

#[tokio::test]
async fn init_is_cross_instance_atomic_and_never_clobbers_existing_records() {
    let directory = tempfile::tempdir().unwrap();
    let sentinel_path = directory.path().join("existing.json");
    std::fs::write(&sentinel_path, b"sentinel").unwrap();
    let store = JsonStateStore::new(directory.path());
    assert_eq!(
        store
            .init_run("existing", "flow", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );
    assert_eq!(std::fs::read(&sentinel_path).unwrap(), b"sentinel");

    let first = JsonStateStore::new(directory.path());
    let second = JsonStateStore::new(directory.path());
    let first_context = Context::new();
    let second_context = Context::new();
    let (first_result, second_result) = tokio::join!(
        first.init_run("race", "first", &first_context),
        second.init_run("race", "second", &second_context)
    );
    assert_eq!(
        [first_result.is_ok(), second_result.is_ok()]
            .into_iter()
            .filter(|success| *success)
            .count(),
        1
    );
    let loser = first_result.err().or_else(|| second_result.err()).unwrap();
    assert_eq!(loser.kind(), StorageErrorKind::Conflict);
    let stored = first.get_run_info("race").await.unwrap();
    assert!(matches!(stored.flow_name.as_str(), "first" | "second"));
    assert_no_temporary_entries(directory.path());
}

#[tokio::test]
async fn atomic_updates_leave_complete_records_and_no_temporary_files() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("updated", "flow", &HashMap::new())
        .await
        .unwrap();
    store
        .set_run_status("updated", RunStatus::Success)
        .await
        .unwrap();

    let run: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("updated.json")).unwrap())
            .unwrap();
    let summary: serde_json::Value = serde_json::from_slice(
        &std::fs::read(directory.path().join("updated.summary.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(run["id"], "updated");
    assert_eq!(summary["id"], "updated");
    assert_no_temporary_entries(directory.path());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_raw_readers_never_observe_partial_replacements() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(directory.path()));
    store
        .init_run("observed", "flow", &Context::new())
        .await
        .unwrap();

    let run_path = directory.path().join("observed.json");
    let finished = Arc::new(AtomicBool::new(false));
    let updates_active = Arc::new(AtomicBool::new(false));
    let reader_updates_active = updates_active.clone();
    let reader_finished = finished.clone();
    let (first_read_tx, first_read_rx) = tokio::sync::oneshot::channel();
    let (overlap_tx, overlap_rx) = tokio::sync::oneshot::channel();
    let reader = tokio::spawn(async move {
        let mut reads = 0;
        let mut first_read_tx = Some(first_read_tx);
        let mut overlap_tx = Some(overlap_tx);
        let mut overlap_reads = 0;
        while !reader_finished.load(Ordering::Acquire) || reads == 0 {
            let raw = tokio::fs::read(&run_path).await.unwrap();
            let value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
            assert_eq!(value["id"], "observed");
            reads += 1;
            if let Some(sender) = first_read_tx.take() {
                sender.send(()).unwrap();
            }
            if reader_updates_active.load(Ordering::Acquire) {
                overlap_reads += 1;
                if let Some(sender) = overlap_tx.take() {
                    sender.send(()).unwrap();
                }
            }
            tokio::task::yield_now().await;
        }
        (reads, overlap_reads)
    });

    first_read_rx.await.unwrap();
    let mut first_update = Context::new();
    first_update.insert("revision".to_string(), serde_json::json!(0));
    store.update_ctx("observed", &first_update).await.unwrap();
    updates_active.store(true, Ordering::Release);
    tokio::time::timeout(std::time::Duration::from_secs(5), overlap_rx)
        .await
        .unwrap()
        .unwrap();
    for revision in 1..48 {
        let mut update = Context::new();
        update.insert("revision".to_string(), serde_json::json!(revision));
        store.update_ctx("observed", &update).await.unwrap();
    }
    updates_active.store(false, Ordering::Release);
    finished.store(true, Ordering::Release);
    let (reads, overlap_reads) = reader.await.unwrap();
    assert!(reads > 0);
    assert!(overlap_reads > 0);
    assert_no_temporary_entries(directory.path());
}

#[tokio::test]
async fn listings_reject_invalid_filenames_and_payload_id_mismatches() {
    let invalid_name_dir = tempfile::tempdir().unwrap();
    std::fs::write(invalid_name_dir.path().join("bad!.json"), b"{}").unwrap();
    let store = JsonStateStore::new(invalid_name_dir.path());
    assert_eq!(
        store.list_runs(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );

    let mismatched_run_dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(mismatched_run_dir.path());
    store
        .init_run("expected", "flow", &Context::new())
        .await
        .unwrap();
    let run_path = mismatched_run_dir.path().join("expected.json");
    let mut run: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&run_path).unwrap()).unwrap();
    run["id"] = serde_json::json!("different");
    std::fs::write(&run_path, serde_json::to_vec(&run).unwrap()).unwrap();
    assert_eq!(
        store.list_runs(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );

    let mismatched_summary_dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(mismatched_summary_dir.path());
    store
        .init_run("expected", "flow", &Context::new())
        .await
        .unwrap();
    let summary_path = mismatched_summary_dir.path().join("expected.summary.json");
    let mut summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&summary_path).unwrap()).unwrap();
    summary["id"] = serde_json::json!("different");
    std::fs::write(&summary_path, serde_json::to_vec(&summary).unwrap()).unwrap();
    assert_eq!(
        store.list_run_summaries(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
}

#[tokio::test]
async fn matching_non_regular_entries_are_rejected() {
    let run_directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(run_directory.path().join("directory.json")).unwrap();
    let store = JsonStateStore::new(run_directory.path());
    assert_eq!(
        store.list_runs(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );

    let summary_directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(summary_directory.path().join("orphan.summary.json")).unwrap();
    let store = JsonStateStore::new(summary_directory.path());
    assert_eq!(
        store.list_run_summaries(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
}

#[cfg(unix)]
#[path = "json_store_security/unix.rs"]
mod unix_security;
