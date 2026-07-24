use std::collections::HashMap;

use crate::engine::types::{Context, RunStatus, TaskState};
use crate::storage::{PageSize, RunCursor, RunListQuery, StateStore};

use super::delta::DELTA_NAME;
use super::state;
use super::{CATALOG_NAME, STATE_NAME};
use crate::storage::json_store::JsonStateStore;

fn query(status: Option<RunStatus>, after: Option<RunCursor>, limit: usize) -> RunListQuery {
    RunListQuery::new(status, after, PageSize::new(limit).unwrap()).unwrap()
}

#[tokio::test]
async fn clean_first_and_deep_pages_never_enumerate_the_store_directory() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    for index in 0..80 {
        store
            .init_run(&format!("bounded-{index:03}"), "flow", &Context::new())
            .await
            .unwrap();
    }

    store.reset_catalog_read_counters();
    let first = store
        .list_run_summaries_page(&query(None, None, 3))
        .await
        .unwrap();
    assert_eq!(first.items.len(), 3);
    assert_eq!(store.catalog_read_counters(), (0, 4));

    let mut after = first.next;
    for _ in 0..10 {
        let page = store
            .list_run_summaries_page(&query(None, after, 3))
            .await
            .unwrap();
        after = page.next;
    }
    store.reset_catalog_read_counters();
    let deep = store
        .list_run_summaries_page(&query(None, after, 3))
        .await
        .unwrap();
    assert_eq!(deep.items.len(), 3);
    assert_eq!(store.catalog_read_counters(), (0, 4));
}

#[tokio::test]
async fn status_sections_track_transitions_upserts_and_deletes() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    let statuses = [
        RunStatus::Pending,
        RunStatus::Running,
        RunStatus::Success,
        RunStatus::Failed,
        RunStatus::Stalled,
        RunStatus::Cancelled,
    ];
    for (index, status) in statuses.iter().enumerate() {
        let id = format!("status-{index}");
        store.init_run(&id, "flow", &Context::new()).await.unwrap();
        if status != &RunStatus::Pending {
            store.set_run_status(&id, status.clone()).await.unwrap();
        }
    }
    for status in &statuses {
        let page = store
            .list_run_summaries_page(&query(Some(status.clone()), None, 10))
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1, "missing {status} section member");
        assert_eq!(&page.items[0].status, status);
    }

    let task = TaskState::new("step", "log");
    store.upsert_task("status-1", &task).await.unwrap();
    let running = store
        .list_run_summaries_page(&query(Some(RunStatus::Running), None, 10))
        .await
        .unwrap();
    assert_eq!(running.items[0].task_count, 1);

    store
        .set_run_status("status-0", RunStatus::Success)
        .await
        .unwrap();
    assert!(
        store
            .list_run_summaries_page(&query(Some(RunStatus::Pending), None, 10))
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(
        store
            .list_run_summaries_page(&query(Some(RunStatus::Success), None, 10))
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    store.delete_run("status-3").await.unwrap();
    assert!(
        store
            .list_run_summaries_page(&query(Some(RunStatus::Failed), None, 10))
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn task_and_context_updates_do_not_replace_the_ordered_projection() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("projection-stable", "flow", &Context::new())
        .await
        .unwrap();
    let catalog_path = directory.path().join(CATALOG_NAME);
    let delta_path = directory.path().join(DELTA_NAME);
    let original = std::fs::read(&catalog_path).unwrap();
    let original_delta = std::fs::read(&delta_path).unwrap();
    let original_modified = std::fs::metadata(&catalog_path)
        .unwrap()
        .modified()
        .unwrap();

    store
        .upsert_task("projection-stable", &TaskState::new("step", "log"))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&catalog_path).unwrap(), original);
    assert_eq!(std::fs::read(&delta_path).unwrap(), original_delta);
    let mut update = HashMap::new();
    update.insert("answer".to_string(), serde_json::json!(42));
    store
        .update_ctx("projection-stable", &update)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&catalog_path).unwrap(), original);
    assert_eq!(std::fs::read(&delta_path).unwrap(), original_delta);
    assert_eq!(
        std::fs::metadata(&catalog_path)
            .unwrap()
            .modified()
            .unwrap(),
        original_modified
    );

    let page = store
        .list_run_summaries_page(&query(None, None, 1))
        .await
        .unwrap();
    assert_eq!(page.items[0].task_count, 1);
    assert_eq!(
        store.get_ctx("projection-stable").await.unwrap()["answer"],
        42
    );
}

#[tokio::test]
async fn missing_corrupt_and_dirty_catalogs_rebuild_from_primary_records() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    for id in ["recover-a", "recover-b"] {
        store.init_run(id, "flow", &Context::new()).await.unwrap();
    }

    std::fs::remove_file(directory.path().join(CATALOG_NAME)).unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&query(None, None, 10))
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    std::fs::write(directory.path().join(CATALOG_NAME), b"broken catalog").unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&query(None, None, 10))
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    std::fs::remove_file(directory.path().join(DELTA_NAME)).unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&query(None, None, 10))
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    std::fs::write(directory.path().join(DELTA_NAME), b"broken delta").unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&query(None, None, 10))
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    state::mark_dirty(&store.directory).await.unwrap();
    assert!(
        state::current_token(&store.directory)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .list_run_summaries_page(&query(None, None, 10))
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    std::fs::remove_file(directory.path().join(STATE_NAME)).unwrap();
    assert_eq!(store.rebuild_run_summary_catalog().await.unwrap(), 2);
}

#[tokio::test]
async fn primary_and_summary_revision_drift_repair_without_stale_pages() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("revision-drift", "flow", &Context::new())
        .await
        .unwrap();
    let run_path = directory.path().join("revision-drift.json");
    let summary_path = directory.path().join("revision-drift.summary.json");
    let pending_primary = std::fs::read(&run_path).unwrap();
    let pending_summary = std::fs::read(&summary_path).unwrap();

    store
        .set_run_status("revision-drift", RunStatus::Success)
        .await
        .unwrap();
    std::fs::write(&run_path, &pending_primary).unwrap();
    std::fs::write(&summary_path, &pending_summary).unwrap();
    let page = store
        .list_run_summaries_page(&query(None, None, 10))
        .await
        .unwrap();
    assert_eq!(page.items[0].status, RunStatus::Pending);

    store
        .set_run_status("revision-drift", RunStatus::Success)
        .await
        .unwrap();
    std::fs::write(&summary_path, pending_summary).unwrap();
    let page = store
        .list_run_summaries_page(&query(None, None, 10))
        .await
        .unwrap();
    assert_eq!(page.items[0].status, RunStatus::Success);
    let repaired: serde_json::Value =
        serde_json::from_slice(&std::fs::read(summary_path).unwrap()).unwrap();
    assert_eq!(repaired["status"], "success");
}
