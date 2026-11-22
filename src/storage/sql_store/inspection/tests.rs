use std::time::Duration;

use super::*;
use crate::engine::types::{Context, RunStatus, TaskState};
use crate::storage::{SqliteJournalMode, StateStore, StorageErrorKind};

/// Upper bound for any wait in these tests; SQLite's busy timeout is 5 s.
const BOUND: Duration = Duration::from_secs(15);

async fn seed(store: &SqlStateStore, id: &str, generation: &str) {
    let ctx = Context::from([("generation".into(), generation.into())]);
    store.init_run(id, generation, &ctx).await.unwrap();
    store
        .upsert_task(id, &TaskState::new(generation, "log"))
        .await
        .unwrap();
    store.set_run_status(id, RunStatus::Success).await.unwrap();
}

fn assert_generation(run: RunInfo, generation: &str) {
    assert_eq!(run.flow_name, generation);
    assert_eq!(run.ctx["generation"], generation);
    assert_eq!(run.tasks.len(), 1);
    assert!(
        run.tasks.contains_key(generation),
        "mixed generations: {:?}",
        run.tasks.keys()
    );
}

async fn replace(store: &SqlStateStore, id: &str) {
    store.delete_run(id).await.unwrap();
    seed(store, id, "replacement").await;
}

/// Replace a run while an inspection is open, then finish the inspection.
///
/// WAL and PostgreSQL must let the replacement commit before the snapshot
/// reads its tasks, irrespective of how the executor schedules the writer.
async fn replace_between_reads(store: &SqlStateStore) {
    let id = uuid::Uuid::new_v4().to_string();
    seed(store, &id, "original").await;
    let read = store.begin_inspection(&id).await.unwrap();
    tokio::time::timeout(BOUND, replace(store, &id))
        .await
        .expect("replacement must commit beside the open inspection");
    let old = read.finish().await.unwrap();
    let new = store.get_run_info(&id).await.unwrap();
    store.delete_run(&id).await.unwrap();
    assert_generation(old, "original");
    assert_generation(new, "replacement");
}

/// A store in the given journal mode, set by the store itself as
/// `IRONFLOW_SQLITE_WAL` would (IF-267).
async fn sqlite_store(dir: &tempfile::TempDir, mode: SqliteJournalMode) -> SqlStateStore {
    let url = format!("sqlite://{}/runs.db?mode=rwc", dir.path().display());
    let store = SqlStateStore::new_with_journal_mode(&url, None, mode)
        .await
        .unwrap();
    let modes = journal_modes(&store).await;
    assert!(
        modes.iter().all(|actual| actual == mode.as_str()),
        "{modes:?}"
    );
    store
}

/// The journal mode every pooled connection uses after reading the file.
///
/// An idle connection reports its cached mode until it next reads the
/// database header, so it may still say `delete` just after a WAL switch.
async fn journal_modes(store: &SqlStateStore) -> Vec<String> {
    let mut held = Vec::new();
    let mut modes = Vec::new();
    for _ in 0..store.pool.options().get_max_connections() {
        let mut connection = store.pool.acquire().await.unwrap();
        sqlx::query("SELECT count(*) FROM sqlite_master")
            .execute(&mut *connection)
            .await
            .unwrap();
        modes.push(
            sqlx::query_scalar("PRAGMA journal_mode")
                .fetch_one(&mut *connection)
                .await
                .unwrap(),
        );
        held.push(connection);
    }
    modes
}

/// WAL, the default since IF-267: the replacement commits beside the open
/// snapshot.
#[tokio::test]
async fn sqlite_wal_replacement_between_inspection_reads() {
    let dir = tempfile::tempdir().unwrap();
    let store = sqlite_store(&dir, SqliteJournalMode::Wal).await;
    replace_between_reads(&store).await;
}

/// In WAL a writer commits while an inspection stays open, well inside the
/// busy timeout, and the open snapshot still returns the original run.
#[tokio::test]
async fn sqlite_wal_writer_commits_beside_open_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let store = sqlite_store(&dir, SqliteJournalMode::Wal).await;
    let id = uuid::Uuid::new_v4().to_string();
    seed(&store, &id, "original").await;
    let read = store.begin_inspection(&id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), store.delete_run(&id))
        .await
        .expect("a WAL writer must not wait for an open reader")
        .expect("a WAL writer commits beside an open reader");
    assert_generation(read.finish().await.unwrap(), "original");
    let error = store.get_run_info(&id).await.unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::NotFound);
}

/// Under a rollback journal a writer cannot commit past an inspection that
/// never finishes. It must fail within SQLite's busy timeout, leave the run
/// intact, and must not disturb the open snapshot.
#[tokio::test]
async fn sqlite_rollback_journal_writer_fails_bounded_behind_open_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let store = sqlite_store(&dir, SqliteJournalMode::Delete).await;
    let id = uuid::Uuid::new_v4().to_string();
    seed(&store, &id, "original").await;
    let read = store.begin_inspection(&id).await.unwrap();
    let error = tokio::time::timeout(BOUND, store.delete_run(&id))
        .await
        .expect("a blocked writer must give up within the busy timeout")
        .expect_err("a rollback journal cannot commit past an open reader");
    assert_eq!(error.kind(), StorageErrorKind::Backend);
    assert_generation(read.finish().await.unwrap(), "original");
    assert_generation(store.get_run_info(&id).await.unwrap(), "original");
    replace(&store, &id).await;
    assert_generation(store.get_run_info(&id).await.unwrap(), "replacement");
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_replacement_between_inspection_reads() {
    if std::env::var("IRONFLOW_POSTGRES_TEST_REQUIRED").as_deref() != Ok("1") {
        return;
    }
    let url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| s.starts_with("postgres://") || s.starts_with("postgresql://"))
        .expect("expected a disposable PostgreSQL DATABASE_URL");
    let store = SqlStateStore::new(&url).await.unwrap();
    replace_between_reads(&store).await;
}
