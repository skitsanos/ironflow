use std::sync::Arc;

use super::*;
use crate::storage::StateStore;

struct NotifyOnDrop(Arc<tokio::sync::Notify>);

impl Drop for NotifyOnDrop {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

#[tokio::test]
async fn live_renewal_proceeds_while_expired_backlog_is_reaped() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(directory.path()));
    let expired = RunLease::at("expired-owner", Utc::now() - chrono::Duration::seconds(1));
    for index in 0..32 {
        store
            .init_run_owned(
                &format!("expired-{index}"),
                "flow",
                &Context::new(),
                &expired,
            )
            .await
            .unwrap();
    }
    store
        .init_run_owned(
            "live",
            "flow",
            &Context::new(),
            &RunLease::renewed("live-owner".to_string()),
        )
        .await
        .unwrap();

    let reached = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    *store.lease_reap_hook.lock().unwrap() = Some((reached.clone(), resume.clone()));
    let reaper_store = store.clone();
    let reaper =
        tokio::spawn(async move { reaper_store.reconcile_expired_run_leases(Utc::now()).await });
    tokio::time::timeout(std::time::Duration::from_secs(1), reached.notified())
        .await
        .unwrap();

    let renewal = tokio::time::timeout(
        std::time::Duration::from_millis(250),
        store.renew_run_lease("live", &RunLease::renewed("live-owner".to_string())),
    )
    .await
    .expect("live renewal was blocked by the expired backlog")
    .unwrap();
    assert!(renewal);
    resume.notify_one();
    assert_eq!(reaper.await.unwrap().unwrap(), 32);
}

#[test]
fn cancelled_owned_write_keeps_the_lease_lock_until_commit_with_a_small_blocking_pool() {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(cancelled_owned_write_case());
}

async fn cancelled_owned_write_case() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(directory.path()));
    store
        .init_run_owned(
            "cancelled-write",
            "flow",
            &Context::new(),
            &RunLease::renewed("owner".to_string()),
        )
        .await
        .unwrap();

    let commit_reached = Arc::new(tokio::sync::Notify::new());
    let resume_commit = Arc::new(tokio::sync::Notify::new());
    let _resume_on_drop = NotifyOnDrop(resume_commit.clone());
    *store.lease_commit_hook.lock().unwrap() =
        Some((commit_reached.clone(), resume_commit.clone()));
    let writer_store = store.clone();
    let writer = tokio::spawn(async move {
        writer_store
            .upsert_task_owned(
                "cancelled-write",
                &TaskState::new("durable-task", "log"),
                "owner",
            )
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), commit_reached.notified())
        .await
        .unwrap();
    writer.abort();
    assert!(writer.await.unwrap_err().is_cancelled());

    let lock_attempted = Arc::new(tokio::sync::Notify::new());
    *store.lease_lock_attempt_hook.lock().unwrap() = Some(lock_attempted.clone());
    let terminal_store = store.clone();
    let terminal = tokio::spawn(async move {
        terminal_store
            .set_run_status_owned("cancelled-write", RunStatus::Success, "owner")
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), lock_attempted.notified())
        .await
        .unwrap();
    tokio::task::yield_now().await;
    assert!(
        !terminal.is_finished(),
        "cancelling the caller released the lease lock before its commit"
    );

    resume_commit.notify_one();
    assert!(terminal.await.unwrap().unwrap());
    let info = store.get_run_info("cancelled-write").await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert!(info.tasks.contains_key("durable-task"));
}
