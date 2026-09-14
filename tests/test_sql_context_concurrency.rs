use std::sync::Arc;
use std::time::Duration;

use ironflow::engine::types::Context;
use ironflow::storage::sql_store::SqlStateStore;
use ironflow::storage::{RunLease, StateStore, StorageErrorKind};
use serde_json::json;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

#[cfg(feature = "postgres")]
#[path = "support/sql_context_postgres.rs"]
mod postgres;
#[cfg(feature = "postgres")]
#[path = "support/postgres.rs"]
mod postgres_fixture;

fn context(key: &str, value: serde_json::Value) -> Context {
    Context::from([(key.to_string(), value)])
}

async fn sqlite_stores() -> (
    tempfile::TempDir,
    Arc<SqlStateStore>,
    Arc<SqlStateStore>,
    sqlx::AnyPool,
) {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("state.sqlite").display()
    );
    let first = Arc::new(SqlStateStore::new(&url).await.unwrap());
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&pool)
        .await
        .unwrap();
    let second = Arc::new(SqlStateStore::new(&url).await.unwrap());
    (directory, first, second, pool)
}

async fn concurrent_merges(first: Arc<SqlStateStore>, second: Arc<SqlStateStore>, mixed: bool) {
    const WRITERS: usize = 20;
    for round in 0..6 {
        let run_id = format!("merge-{mixed}-{round}");
        let seed = context("seed", json!({"nested": [1, null, "kept"]}));
        first
            .init_run_owned(&run_id, "flow", &seed, &RunLease::renewed("owner".into()))
            .await
            .unwrap();
        let barrier = Arc::new(Barrier::new(WRITERS + 1));
        let mut writers = JoinSet::new();
        for index in 0..WRITERS {
            let store = Arc::clone(if index % 2 == 0 { &first } else { &second });
            let barrier = Arc::clone(&barrier);
            let run_id = run_id.clone();
            writers.spawn(async move {
                let delta = context(&format!("writer-{index}"), json!(index));
                barrier.wait().await;
                if mixed && index % 2 == 0 {
                    assert!(
                        store
                            .update_ctx_owned(&run_id, &delta, "owner")
                            .await
                            .unwrap()
                    );
                } else {
                    store.update_ctx(&run_id, &delta).await.unwrap();
                }
            });
        }
        barrier.wait().await;
        tokio::time::timeout(Duration::from_secs(15), async {
            while let Some(result) = writers.join_next().await {
                result.unwrap();
            }
        })
        .await
        .expect("context writers must finish");
        let actual = first.get_ctx(&run_id).await.unwrap();
        assert_eq!(actual.get("seed"), seed.get("seed"));
        for index in 0..WRITERS {
            assert_eq!(actual.get(&format!("writer-{index}")), Some(&json!(index)));
        }
        assert_eq!(actual.len(), WRITERS + 1);
        assert!(
            !second
                .update_ctx_owned(&run_id, &context("stale", json!(true)), "stale-owner")
                .await
                .unwrap()
        );
        assert_eq!(first.get_ctx(&run_id).await.unwrap(), actual);
    }
}

async fn merge_semantics(store: &SqlStateStore) {
    assert_eq!(
        store
            .update_ctx("missing", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
    let seed = Context::from([
        ("untouched".into(), json!(1)),
        ("nested".into(), json!({"old": 1})),
        ("nullable".into(), json!("old")),
    ]);
    store.init_run("semantics", "flow", &seed).await.unwrap();
    store
        .update_ctx("semantics", &Context::new())
        .await
        .unwrap();
    assert_eq!(store.get_ctx("semantics").await.unwrap(), seed);
    let delta = Context::from([
        ("nested".into(), json!({"new": [2, 3]})),
        ("nullable".into(), json!(null)),
    ]);
    store.update_ctx("semantics", &delta).await.unwrap();
    let mut expected = seed;
    expected.extend(delta);
    assert_eq!(store.get_ctx("semantics").await.unwrap(), expected);
}

async fn delete_recreate_races(first: Arc<SqlStateStore>, second: Arc<SqlStateStore>) {
    for index in 0..40 {
        let id = format!("recreate-{index}");
        first
            .init_run(&id, "old", &context("old-only", json!(true)))
            .await
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let mut tasks = JoinSet::new();
        let writer_id = id.clone();
        let writer = Arc::clone(&second);
        let start = Arc::clone(&barrier);
        tasks.spawn(async move {
            start.wait().await;
            writer
                .update_ctx(&writer_id, &context("delta", json!(true)))
                .await
        });
        barrier.wait().await;
        first.delete_run(&id).await.unwrap();
        first
            .init_run(&id, "new", &context("new-only", json!(true)))
            .await
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(10), tasks.join_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Err(error) = result {
            assert_eq!(error.kind(), StorageErrorKind::NotFound);
        }
        let actual = first.get_ctx(&id).await.unwrap();
        assert_eq!(actual.get("new-only"), Some(&json!(true)));
        assert!(
            !actual.contains_key("old-only"),
            "old snapshot crossed recreation: {actual:?}"
        );
        // A writer whose first locked read follows recreation can apply its delta.
        assert!(actual.len() <= 2);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_concurrent_unowned_merges_preserve_disjoint_updates() {
    let (_directory, first, second, _pool) = sqlite_stores().await;
    concurrent_merges(first, second, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_owned_and_unowned_merges_preserve_disjoint_updates() {
    let (_directory, first, second, _pool) = sqlite_stores().await;
    concurrent_merges(first, second, true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_context_snapshot_cannot_cross_delete_recreate() {
    let (_directory, first, second, _pool) = sqlite_stores().await;
    delete_recreate_races(first, second).await;
}

#[tokio::test]
async fn sqlite_context_merge_semantics_and_failure_rollback() {
    let (_directory, store, _second, pool) = sqlite_stores().await;
    merge_semantics(&store).await;
    sqlx::query("UPDATE ironflow_runs SET ctx = '{broken' WHERE id = 'semantics'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .update_ctx("semantics", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    sqlx::query("UPDATE ironflow_runs SET ctx = '{}' WHERE id = 'semantics'")
        .execute(&pool)
        .await
        .unwrap();
    // A trigger writes first and then rejects the statement: neither write may commit.
    sqlx::query("CREATE TRIGGER reject_context BEFORE UPDATE OF ctx ON ironflow_runs BEGIN UPDATE ironflow_runs SET flow_name = 'changed' WHERE id = NEW.id; SELECT RAISE(FAIL, 'injected context failure'); END")
        .execute(&pool).await.unwrap();
    assert!(
        store
            .update_ctx("semantics", &context("rejected", json!(true)))
            .await
            .is_err()
    );
    assert_eq!(
        store.get_run_info("semantics").await.unwrap().flow_name,
        "flow"
    );
    assert!(store.get_ctx("semantics").await.unwrap().is_empty());
    sqlx::query("DROP TRIGGER reject_context")
        .execute(&pool)
        .await
        .unwrap();
    store
        .update_ctx("semantics", &context("recovered", json!(true)))
        .await
        .unwrap();
}

#[tokio::test]
async fn sqlite_cancelled_context_writer_releases_transaction() {
    let (_directory, store, _second, pool) = sqlite_stores().await;
    store
        .init_run("cancel", "flow", &Context::new())
        .await
        .unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("UPDATE ironflow_runs SET id = id WHERE id = 'cancel'")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let delta = context("cancelled", json!(true));
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            store.update_ctx("cancel", &delta)
        )
        .await
        .is_err()
    );
    blocker.rollback().await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(5),
        store.update_ctx("cancel", &context("after", json!(1))),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        store.get_ctx("cancel").await.unwrap(),
        context("after", json!(1))
    );
}
