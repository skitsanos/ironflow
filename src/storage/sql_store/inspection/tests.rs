use std::time::Duration;

use super::*;
use crate::engine::types::{Context, RunStatus, TaskState};
use crate::storage::{StateStore, StorageErrorKind};

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
/// WAL and PostgreSQL let the replacement commit beside the snapshot; a
/// rollback journal makes it wait for the reader. Either way the open
/// inspection returns only the original and the next only the replacement.
async fn replace_between_reads(store: &SqlStateStore) {
    let id = uuid::Uuid::new_v4().to_string();
    seed(store, &id, "original").await;
    let read = store.begin_inspection(&id).await.unwrap();
    let inspect = async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        read.finish().await.unwrap()
    };
    let (old, ()) =
        tokio::time::timeout(BOUND, async { tokio::join!(inspect, replace(store, &id)) })
            .await
            .expect("inspection and replacement must both finish");
    let new = store.get_run_info(&id).await.unwrap();
    store.delete_run(&id).await.unwrap();
    assert_generation(old, "original");
    assert_generation(new, "replacement");
}

async fn sqlite_store(dir: &tempfile::TempDir) -> SqlStateStore {
    let url = format!("sqlite://{}/runs.db?mode=rwc", dir.path().display());
    SqlStateStore::new(&url).await.unwrap()
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

/// The store sets no journal mode, so a new database keeps SQLite's default
/// rollback journal: the mode IronFlow runs in unless an operator converted
/// the file to WAL.
#[tokio::test]
async fn sqlite_default_journal_replacement_between_inspection_reads() {
    let dir = tempfile::tempdir().unwrap();
    let store = sqlite_store(&dir).await;
    let modes = journal_modes(&store).await;
    assert!(modes.iter().all(|mode| mode == "delete"), "{modes:?}");
    replace_between_reads(&store).await;
}

/// A database file converted to WAL stays WAL for every later connection.
#[tokio::test]
async fn sqlite_wal_replacement_between_inspection_reads() {
    let dir = tempfile::tempdir().unwrap();
    let store = sqlite_store(&dir).await;
    let set: String = sqlx::query_scalar("PRAGMA journal_mode=WAL")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(set, "wal");
    assert!(journal_modes(&store).await.iter().all(|mode| mode == "wal"));
    replace_between_reads(&store).await;
}

/// Under a rollback journal a writer cannot commit past an inspection that
/// never finishes. It must fail within SQLite's busy timeout, leave the run
/// intact, and must not disturb the open snapshot.
#[tokio::test]
async fn sqlite_default_journal_writer_fails_bounded_behind_open_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let store = sqlite_store(&dir).await;
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
