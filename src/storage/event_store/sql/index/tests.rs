use sqlx::any::AnyPoolOptions;

use super::*;
use crate::storage::sql_names::SqlEventTableNames;

async fn primed_store() -> (tempfile::TempDir, SqlEventStore) {
    sqlx::any::install_default_drivers();
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite:{}?mode=rwc",
        directory.path().join("events.sqlite").display()
    );
    let pool = AnyPoolOptions::new()
        .min_connections(4)
        .max_connections(4)
        .connect(&url)
        .await
        .unwrap();
    let mut connections = Vec::new();
    for _ in 0..4 {
        connections.push(pool.acquire().await.unwrap());
    }
    // Cache the empty database schema before another connection creates it.
    for connection in &mut connections {
        sqlx::query("SELECT name FROM sqlite_master")
            .fetch_all(&mut **connection)
            .await
            .unwrap();
        sqlx::query("PRAGMA index_list('ironflow_events')")
            .fetch_all(&mut **connection)
            .await
            .unwrap();
        sqlx::query("PRAGMA index_xinfo('ironflow_events_run_seq_idx')")
            .fetch_all(&mut **connection)
            .await
            .unwrap();
    }
    sqlx::query("CREATE TABLE ironflow_events (run_id TEXT, sequence BIGINT)")
        .execute(&mut *connections[0])
        .await
        .unwrap();
    drop(connections);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while pool.num_idle() != 4 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("primed connections must return to the pool");
    let store = SqlEventStore {
        pool,
        tables: SqlEventTableNames::new(None).unwrap(),
        dialect: SqlDialect::Sqlite,
    };
    (directory, store)
}

#[tokio::test]
async fn newly_created_sqlite_sequence_index_survives_stale_pool_schemas() {
    let (_directory, store) = primed_store().await;
    store.ensure_event_sequence_index().await.unwrap();
    store.pool.close().await;
}

#[tokio::test]
async fn sqlite_index_verification_still_rejects_incompatible_definitions() {
    for definition in [
        "CREATE INDEX ironflow_events_run_seq_idx ON ironflow_events(run_id, sequence)",
        "CREATE UNIQUE INDEX ironflow_events_run_seq_idx ON ironflow_events(run_id, sequence) WHERE sequence IS NOT NULL",
        "CREATE UNIQUE INDEX ironflow_events_run_seq_idx ON ironflow_events(sequence, run_id)",
        "CREATE UNIQUE INDEX ironflow_events_run_seq_idx ON ironflow_events(run_id, sequence + 1)",
    ] {
        let (_directory, store) = primed_store().await;
        sqlx::query(sqlx::AssertSqlSafe(definition))
            .execute(&store.pool)
            .await
            .unwrap();
        let error = store.ensure_event_sequence_index().await.unwrap_err();
        assert_eq!(error.kind(), crate::storage::StorageErrorKind::Conflict);
        store.pool.close().await;
    }
}
