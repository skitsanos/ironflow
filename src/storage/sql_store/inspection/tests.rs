use super::*;
use crate::engine::types::{Context, RunStatus, TaskState};
use crate::storage::StateStore;

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

async fn replace_between_reads(store: &SqlStateStore) {
    let id = uuid::Uuid::new_v4().to_string();
    seed(store, &id, "original").await;
    let read = store.begin_inspection(&id).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        store.delete_run(&id).await.unwrap();
        seed(store, &id, "replacement").await;
    })
    .await
    .expect("an inspection must not block replacement in WAL or MVCC mode");
    let old = read.finish().await.unwrap();
    let new = store.get_run_info(&id).await.unwrap();
    store.delete_run(&id).await.unwrap();
    assert_generation(old, "original");
    assert_generation(new, "replacement");
}

#[tokio::test]
async fn sqlite_replacement_between_inspection_reads() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}/runs.db?mode=rwc", dir.path().display());
    let store = SqlStateStore::new(&url).await.unwrap();
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&store.pool)
        .await
        .unwrap();
    replace_between_reads(&store).await;
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
