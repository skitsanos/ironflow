//! IF-184: a deleted run's event fence belongs to the deleted incarnation.

use std::sync::Arc;

use ironflow::storage::event_store::{MemoryEventStore, SqlEventStore};

#[path = "event_store/fence_contract.rs"]
mod fence_contract;

async fn sqlite_events(directory: &std::path::Path) -> SqlEventStore {
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.join("fence-events.sqlite").to_string_lossy()
    );
    SqlEventStore::new(&url).await.unwrap()
}

#[tokio::test]
async fn memory_resubmitted_run_has_complete_events() {
    fence_contract::assert_resubmitted_run_has_complete_events(
        Arc::new(MemoryEventStore::new()),
        "memory-resubmit",
    )
    .await;
}

#[tokio::test]
async fn memory_missing_run_delete_leaves_no_fence() {
    fence_contract::assert_missing_run_delete_leaves_no_fence(
        &MemoryEventStore::new(),
        "memory-missing-run",
    )
    .await;
}

#[tokio::test]
async fn sqlite_resubmitted_run_has_complete_events() {
    let directory = tempfile::tempdir().unwrap();
    fence_contract::assert_resubmitted_run_has_complete_events(
        Arc::new(sqlite_events(directory.path()).await),
        "sqlite-resubmit",
    )
    .await;
}

#[tokio::test]
async fn sqlite_missing_run_delete_leaves_no_fence() {
    let directory = tempfile::tempdir().unwrap();
    fence_contract::assert_missing_run_delete_leaves_no_fence(
        &sqlite_events(directory.path()).await,
        "sqlite-missing-run",
    )
    .await;
}

#[tokio::test]
async fn memory_fence_is_scoped_to_one_incarnation() {
    fence_contract::assert_fence_is_scoped_to_one_incarnation(
        &MemoryEventStore::new(),
        "memory-incarnations",
    )
    .await;
}

#[tokio::test]
async fn sqlite_fence_is_scoped_to_one_incarnation() {
    let directory = tempfile::tempdir().unwrap();
    fence_contract::assert_fence_is_scoped_to_one_incarnation(
        &sqlite_events(directory.path()).await,
        "sqlite-incarnations",
    )
    .await;
}

/// A fence table written by an earlier release has no `incarnation` column.
/// Opening the store adds it, keeps the existing fence closed and lets the
/// run ID be admitted again.
#[tokio::test]
async fn sqlite_fence_table_from_an_earlier_release_is_upgraded() {
    use ironflow::storage::StorageErrorKind;
    use ironflow::storage::event_store::EventStore as _;

    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory
            .path()
            .join("fence-events.sqlite")
            .to_string_lossy()
    );
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    for sql in [
        "CREATE TABLE ironflow_event_deletions (run_id TEXT PRIMARY KEY)",
        "INSERT INTO ironflow_event_deletions (run_id) VALUES ('deleted-before-upgrade')",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool.close().await;

    let store = sqlite_events(directory.path()).await;
    // A second open finds the column and changes nothing.
    drop(sqlite_events(directory.path()).await);
    let run_id = "deleted-before-upgrade";
    let event = |id: &str| {
        let mut event = ironflow::engine::RunEvent::run(
            run_id,
            "flow",
            ironflow::engine::RunEventType::RunStarted,
            ironflow::engine::types::RunStatus::Running,
        );
        event.id = id.to_string();
        event
    };
    for rejected in [
        store.publish(event("unscoped")).await,
        store.publish_as(event("late"), "old").await,
    ] {
        assert_eq!(rejected.unwrap_err().kind(), StorageErrorKind::Conflict);
    }
    store.begin_incarnation(run_id, "new").await.unwrap();
    store.publish_as(event("new-1"), "new").await.unwrap();
    assert_eq!(store.list_since(run_id, None, 10).await.unwrap().len(), 1);
}

#[cfg(feature = "postgres")]
mod postgres {
    use std::sync::Arc;

    use ironflow::storage::event_store::SqlEventStore;

    use super::fence_contract;

    /// Explicit opt-in: these tests create and drop prefixed tables.
    fn database_url() -> Option<String> {
        std::env::var("DATABASE_URL")
            .ok()
            .filter(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"))
    }

    async fn drop_tables(url: &str, prefix: &str) {
        let pool = sqlx::AnyPool::connect(url).await.unwrap();
        for table in ["events", "event_sequences", "event_deletions"] {
            let sql = format!("DROP TABLE IF EXISTS {prefix}{table}");
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .execute(&pool)
                .await
                .unwrap();
        }
        pool.close().await;
    }

    #[tokio::test]
    async fn postgres_fence_follows_run_incarnations() {
        let Some(url) = database_url() else {
            eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
            return;
        };
        let id = uuid::Uuid::new_v4().simple().to_string();
        let prefix = format!("if184_{}_", &id[..8]);

        // Start from a fence table written by an earlier release.
        sqlx::any::install_default_drivers();
        let pool = sqlx::AnyPool::connect(&url).await.unwrap();
        let legacy = format!("CREATE TABLE {prefix}event_deletions (run_id TEXT PRIMARY KEY)");
        sqlx::query(sqlx::AssertSqlSafe(legacy))
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let store = Arc::new(
            SqlEventStore::new_with_prefix(&url, Some(&prefix))
                .await
                .unwrap(),
        );
        fence_contract::assert_fence_is_scoped_to_one_incarnation(
            store.as_ref(),
            "postgres-incarnations",
        )
        .await;
        fence_contract::assert_missing_run_delete_leaves_no_fence(
            store.as_ref(),
            "postgres-missing-run",
        )
        .await;
        fence_contract::assert_resubmitted_run_has_complete_events(store, "postgres-resubmit")
            .await;
        drop_tables(&url, &prefix).await;
    }
}
