use crate::engine::types::Context;
use crate::storage::{PageSize, RunListQuery, StateStore, StorageErrorKind};

use super::delta::DELTA_NAME;
use super::state;
use super::{CATALOG_NAME, LOCK_NAME, STATE_NAME};
use crate::storage::json_store::JsonStateStore;

fn query(limit: usize) -> RunListQuery {
    RunListQuery::new(None, None, PageSize::new(limit).unwrap()).unwrap()
}

#[tokio::test]
async fn interrupted_delete_is_removed_by_the_next_catalog_rebuild() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("crash-delete", "flow", &Context::new())
        .await
        .unwrap();
    state::mark_dirty(&store.directory).await.unwrap();
    std::fs::remove_file(directory.path().join("crash-delete.summary.json")).unwrap();
    std::fs::remove_file(directory.path().join("crash-delete.json")).unwrap();
    assert!(
        store
            .list_run_summaries_page(&query(10))
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn two_store_instances_preserve_every_catalog_member() {
    let directory = tempfile::tempdir().unwrap();
    let first = JsonStateStore::new(directory.path());
    let second = JsonStateStore::new(directory.path());
    let first_writer = async {
        for index in 0..10 {
            first
                .init_run(&format!("first-{index}"), "flow", &Context::new())
                .await
                .unwrap();
        }
    };
    let second_writer = async {
        for index in 0..10 {
            second
                .init_run(&format!("second-{index}"), "flow", &Context::new())
                .await
                .unwrap();
        }
    };
    tokio::join!(first_writer, second_writer);
    let page = first.list_run_summaries_page(&query(25)).await.unwrap();
    assert_eq!(page.items.len(), 20);
    assert!(!page.has_more());
    assert_eq!(second.rebuild_run_summary_catalog().await.unwrap(), 20);
}

#[tokio::test]
async fn concurrent_delete_after_catalog_selection_retries_instead_of_leaking_not_found() {
    let directory = tempfile::tempdir().unwrap();
    let reader = std::sync::Arc::new(JsonStateStore::new(directory.path()));
    let writer = JsonStateStore::new(directory.path());
    reader
        .init_run("delete-race", "flow", &Context::new())
        .await
        .unwrap();
    let (entered, resume) = reader.install_catalog_read_hook();
    let read_store = reader.clone();
    let read = tokio::spawn(async move {
        read_store
            .list_run_summaries_page(&query(10))
            .await
            .unwrap()
    });
    entered.notified().await;
    writer.delete_run("delete-race").await.unwrap();
    resume.notify_one();

    let page = read.await.unwrap();
    assert!(page.items.is_empty());
}

#[tokio::test]
async fn concurrent_readers_adopt_a_new_status_generation_without_rebuild_conflicts() {
    let directory = tempfile::tempdir().unwrap();
    let writer = JsonStateStore::new(directory.path());
    writer
        .init_run("status-race", "flow", &Context::new())
        .await
        .unwrap();
    let first = std::sync::Arc::new(JsonStateStore::new(directory.path()));
    let second = std::sync::Arc::new(JsonStateStore::new(directory.path()));
    let (first_entered, first_resume) = first.install_catalog_read_hook();
    let (second_entered, second_resume) = second.install_catalog_read_hook();
    let first_read =
        tokio::spawn(async move { first.list_run_summaries_page(&query(10)).await.unwrap() });
    let second_read =
        tokio::spawn(async move { second.list_run_summaries_page(&query(10)).await.unwrap() });
    first_entered.notified().await;
    second_entered.notified().await;
    writer
        .set_run_status("status-race", crate::engine::types::RunStatus::Success)
        .await
        .unwrap();
    first_resume.notify_one();
    second_resume.notify_one();

    for page in [first_read.await.unwrap(), second_read.await.unwrap()] {
        assert_eq!(
            page.items[0].status,
            crate::engine::types::RunStatus::Success
        );
    }
}

#[tokio::test]
async fn a_rebuild_invalidated_before_write_preparation_retries_without_panicking() {
    let directory = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(JsonStateStore::new(directory.path()));
    store
        .init_run("rebuild-race", "flow", &Context::new())
        .await
        .unwrap();
    state::mark_dirty(&store.directory).await.unwrap();
    let (entered, resume) = store.install_catalog_rebuild_hook();
    let writer = store.clone();
    let update = tokio::spawn(async move {
        let mut context = Context::new();
        context.insert("updated".to_string(), serde_json::json!(true));
        writer.update_ctx("rebuild-race", &context).await
    });
    entered.notified().await;
    std::fs::write(
        directory.path().join("external-marker"),
        b"changed directory",
    )
    .unwrap();
    resume.notify_one();

    update.await.unwrap().unwrap();
    assert_eq!(
        store.get_ctx("rebuild-race").await.unwrap()["updated"],
        true
    );
}

#[tokio::test]
async fn truncated_and_checksum_invalid_state_metadata_rebuilds_automatically() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("state-repair", "flow", &Context::new())
        .await
        .unwrap();
    let state_path = directory.path().join(STATE_NAME);

    std::fs::write(&state_path, b"truncated").unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&query(10))
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    let mut state_data = std::fs::read(&state_path).unwrap();
    let last = state_data.last_mut().unwrap();
    *last ^= 0xff;
    std::fs::write(&state_path, state_data).unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&query(10))
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(
        state::current_token(&store.directory)
            .await
            .unwrap()
            .is_some()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn state_and_lock_symlinks_are_rejected_without_following_them() {
    use std::os::unix::fs::symlink;

    for metadata_name in [STATE_NAME, LOCK_NAME] {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), b"outside sentinel").unwrap();
        let store = JsonStateStore::new(directory.path());
        store
            .init_run("metadata-symlink", "flow", &Context::new())
            .await
            .unwrap();
        std::fs::remove_file(directory.path().join(metadata_name)).unwrap();
        symlink(outside.path(), directory.path().join(metadata_name)).unwrap();
        if metadata_name == LOCK_NAME {
            std::fs::write(directory.path().join(STATE_NAME), b"force rebuild").unwrap();
        }

        let error = store.list_run_summaries_page(&query(10)).await.unwrap_err();
        assert_eq!(error.kind(), StorageErrorKind::Corruption);
        assert_eq!(std::fs::read(outside.path()).unwrap(), b"outside sentinel");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn catalog_and_delta_symlinks_are_rejected_without_following_them() {
    use std::os::unix::fs::symlink;

    for metadata_name in [CATALOG_NAME, DELTA_NAME] {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), b"outside sentinel").unwrap();
        let store = JsonStateStore::new(directory.path());
        store
            .init_run("symlink-catalog", "flow", &Context::new())
            .await
            .unwrap();
        std::fs::remove_file(directory.path().join(metadata_name)).unwrap();
        symlink(outside.path(), directory.path().join(metadata_name)).unwrap();

        let error = store.list_run_summaries_page(&query(10)).await.unwrap_err();
        assert_eq!(error.kind(), StorageErrorKind::Corruption);
        assert_eq!(std::fs::read(outside.path()).unwrap(), b"outside sentinel");
    }
}
