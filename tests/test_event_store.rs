use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::event_store::{EventStore, MemoryEventStore, SqlEventStore};
use sqlx::Row;

#[tokio::test]
async fn memory_event_store_lists_events_after_cursor() {
    let store = MemoryEventStore::new();
    let first = RunEvent::run(
        "run-1",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let second = RunEvent::run(
        "run-1",
        "flow",
        RunEventType::RunFinished,
        RunStatus::Success,
    );

    store.publish(first.clone()).await.unwrap();
    store.publish(second.clone()).await.unwrap();

    let all = store.list_since("run-1", None, 10).await.unwrap();
    assert_eq!(all, vec![first.clone(), second.clone()]);

    let after_first = store
        .list_since("run-1", Some(&first.id), 10)
        .await
        .unwrap();
    assert_eq!(after_first, vec![second]);
}

#[tokio::test]
async fn sqlite_event_store_persists_and_lists_events_after_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("events.sqlite").to_string_lossy()
    );
    let store = SqlEventStore::new(&url).await.unwrap();
    let first = RunEvent::run(
        "run-1",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let second = RunEvent::run(
        "run-1",
        "flow",
        RunEventType::RunFinished,
        RunStatus::Success,
    );

    store.publish(first.clone()).await.unwrap();
    store.publish(second.clone()).await.unwrap();

    let after_first = store
        .list_since("run-1", Some(&first.id), 10)
        .await
        .unwrap();
    assert_eq!(after_first, vec![second]);
}

#[tokio::test]
async fn sqlite_event_store_uses_custom_table_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("events-prefixed.sqlite").to_string_lossy()
    );
    let store = SqlEventStore::new_with_prefix(&url, Some("tenant_a_"))
        .await
        .unwrap();
    let event = RunEvent::run(
        "run-1",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    store.publish(event).await.unwrap();

    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let row = sqlx::query(
        "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'tenant_a_events'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>("count"), 1);
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_event_store_works_with_custom_table_prefix() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_sql_prefix("pg_events");
    let store = SqlEventStore::new_with_prefix(&url, Some(&prefix))
        .await
        .unwrap();
    let first = RunEvent::run(
        "pg-run-1",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let second = RunEvent::run(
        "pg-run-1",
        "flow",
        RunEventType::RunFinished,
        RunStatus::Success,
    );

    store.publish(first.clone()).await.unwrap();
    store.publish(second.clone()).await.unwrap();

    let after_first = store
        .list_since("pg-run-1", Some(&first.id), 10)
        .await
        .unwrap();
    assert_eq!(after_first, vec![second]);

    drop(store);
    cleanup_postgres_event_tables(&url, &prefix).await;
}

#[cfg(feature = "postgres")]
fn postgres_database_url() -> Option<String> {
    dotenvy::dotenv().ok();
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"))
}

#[cfg(feature = "postgres")]
fn unique_sql_prefix(label: &str) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("{}_{}_", label, &id[..8])
}

#[cfg(feature = "postgres")]
async fn cleanup_postgres_event_tables(url: &str, prefix: &str) {
    if let Ok(pool) = sqlx::AnyPool::connect(url).await {
        let _ = sqlx::query(&format!("DROP TABLE IF EXISTS {}events", prefix))
            .execute(&pool)
            .await;
    }
}

#[cfg(feature = "redis")]
#[tokio::test]
async fn redis_event_store_persists_and_lists_events_after_cursor() {
    use ironflow::storage::event_store::RedisEventStore;

    let Ok(store) = RedisEventStore::new(
        "redis://127.0.0.1:6379",
        Some("ironflow_test:event_store:".to_string()),
        Some(60),
    )
    .await
    else {
        eprintln!("Skipping test: Redis not available at 127.0.0.1:6379");
        return;
    };

    let first = RunEvent::run(
        "run-redis-1",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let second = RunEvent::run(
        "run-redis-1",
        "flow",
        RunEventType::RunFinished,
        RunStatus::Success,
    );

    store.publish(first.clone()).await.unwrap();
    store.publish(second.clone()).await.unwrap();

    let all = store.list_since("run-redis-1", None, 10).await.unwrap();
    assert!(all.iter().any(|event| event.id == first.id));
    assert!(all.iter().any(|event| event.id == second.id));

    let after_first = store
        .list_since("run-redis-1", Some(&first.id), 10)
        .await
        .unwrap();
    assert_eq!(after_first, vec![second]);
}
