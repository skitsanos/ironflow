use std::sync::Arc;

use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::StorageErrorKind;
#[cfg(feature = "redis")]
use ironflow::storage::event_store::RedisEventStore;
use ironflow::storage::event_store::{EventStore, MemoryEventStore, SqlEventStore};
use sqlx::Row;

#[path = "event_store/contract.rs"]
mod event_store_contract;
#[cfg(feature = "redis")]
#[path = "support/redis.rs"]
mod redis_support;

use event_store_contract::ordered_event;
use event_store_contract::{assert_event_cursor_contract, assert_event_identity_contract};

fn sqlite_event_url(directory: &std::path::Path, name: &str) -> String {
    format!(
        "sqlite://{}?mode=rwc",
        directory.join(name).to_string_lossy()
    )
}

#[tokio::test]
async fn memory_event_store_classifies_identity_failures() {
    assert_event_identity_contract(&MemoryEventStore::new(), "memory-contract").await;
}

#[tokio::test]
async fn memory_event_store_obeys_cursor_and_batch_contract() {
    let store = MemoryEventStore::new();
    assert_event_cursor_contract(&store, "memory-cursor").await;
}

#[tokio::test]
async fn memory_event_store_bounds_retention_and_deletes_run_events() {
    let store = MemoryEventStore::with_capacity(3).unwrap();
    let first = event_store_contract::ordered_event("memory-a", "first", 1);
    let second = event_store_contract::ordered_event("memory-b", "second", 2);
    let third = event_store_contract::ordered_event("memory-a", "third", 3);
    let fourth = event_store_contract::ordered_event("memory-a", "fourth", 4);

    for event in [&first, &second, &third, &fourth] {
        store.publish(event.clone()).await.unwrap();
    }

    assert_eq!(
        store
            .list_since("memory-a", Some(&first.id), 10)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
    assert_eq!(
        store.list_since("memory-a", None, 10).await.unwrap(),
        vec![third.clone(), fourth.clone()]
    );
    assert_eq!(store.delete_run("memory-a").await.unwrap(), 2);
    assert_eq!(store.delete_run("memory-a").await.unwrap(), 0);
    assert_eq!(
        store.publish(fourth).await.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );
    assert!(
        store
            .list_since("memory-a", None, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store.list_since("memory-b", None, 10).await.unwrap(),
        vec![second]
    );
}

#[tokio::test]
async fn memory_event_store_enforces_its_retained_byte_budget() {
    let store = MemoryEventStore::with_limits(10, 1_024).unwrap();
    let mut first = ordered_event("memory-bytes", "first", 1);
    first.reason = Some("x".repeat(700));
    let mut second = ordered_event("memory-bytes", "second", 2);
    second.reason = Some("y".repeat(700));
    store.publish(first.clone()).await.unwrap();
    store.publish(second.clone()).await.unwrap();

    assert_eq!(
        store
            .list_since("memory-bytes", Some(&first.id), 10)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
    assert_eq!(
        store.list_since("memory-bytes", None, 10).await.unwrap(),
        vec![second]
    );

    let mut oversized = ordered_event("memory-bytes", "oversized", 3);
    oversized.error = Some("z".repeat(2_048));
    assert_eq!(
        store.publish(oversized).await.unwrap_err().kind(),
        StorageErrorKind::InvalidInput
    );
}

#[test]
fn memory_event_store_rejects_an_unbounded_zero_capacity() {
    let error = match MemoryEventStore::with_capacity(0) {
        Ok(_) => panic!("zero event capacity unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), StorageErrorKind::InvalidInput);
}

#[tokio::test]
async fn sqlite_event_store_obeys_cursor_and_batch_contract() {
    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_event_url(dir.path(), "events.sqlite");
    let store = SqlEventStore::new(&url).await.unwrap();
    assert_event_cursor_contract(&store, "sqlite-cursor").await;
}

#[tokio::test]
async fn sqlite_event_store_orders_new_events_by_publication_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_event_url(dir.path(), "event-order.sqlite");
    let store = SqlEventStore::new(&url).await.unwrap();
    let run_id = "sql-publication-order";
    let newest_clock = ordered_event(run_id, "published-first", 30);
    let oldest_clock = ordered_event(run_id, "published-second", -30);
    let middle_clock = ordered_event(run_id, "published-third", 0);

    for event in [&newest_clock, &oldest_clock, &middle_clock] {
        store.publish(event.clone()).await.unwrap();
    }

    assert_eq!(
        store.list_since(run_id, None, 10).await.unwrap(),
        vec![
            newest_clock.clone(),
            oldest_clock.clone(),
            middle_clock.clone()
        ]
    );
    assert_eq!(
        store
            .list_since(run_id, Some(&newest_clock.id), 10)
            .await
            .unwrap(),
        vec![oldest_clock, middle_clock]
    );
}

#[tokio::test]
async fn sqlite_event_store_backfills_legacy_rows_in_bounded_stable_order() {
    const LEGACY_EVENTS: usize = 513;

    sqlx::any::install_default_drivers();
    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_event_url(dir.path(), "event-migration.sqlite");
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_events (\
         id TEXT PRIMARY KEY, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let run_id = "sql-legacy-order";
    let mut expected = Vec::with_capacity(LEGACY_EVENTS);
    for index in 0..LEGACY_EVENTS {
        let event = ordered_event(
            run_id,
            &format!("legacy-{index:04}"),
            i64::try_from(LEGACY_EVENTS - index).unwrap(),
        );
        sqlx::query(
            "INSERT INTO ironflow_events \
             (id, run_id, event_type, event_json, timestamp) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&event.id)
        .bind(&event.run_id)
        .bind(event.event_type.as_sse_name())
        .bind(serde_json::to_string(&event).unwrap())
        .bind(event.timestamp.to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();
        expected.push(event);
    }
    expected.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.id.cmp(&right.id))
    });
    pool.close().await;

    let store = SqlEventStore::new(&url).await.unwrap();
    assert_eq!(
        store
            .list_since(run_id, None, LEGACY_EVENTS + 1)
            .await
            .unwrap(),
        expected
    );

    // The legacy global primary key is upgraded to the public run-scoped
    // identity contract without rewriting existing event IDs.
    let mut reused_id = ordered_event("sql-legacy-other-run", "reused-id", 0);
    reused_id.id.clone_from(&expected[0].id);
    store.publish(reused_id.clone()).await.unwrap();
    assert_eq!(
        store
            .list_since("sql-legacy-other-run", None, 2)
            .await
            .unwrap(),
        vec![reused_id]
    );

    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let query_plan = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT id, run_id FROM ironflow_events \
         WHERE sequence IS NULL ORDER BY run_id, timestamp, id LIMIT 256",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(query_plan.iter().any(|row| {
        row.get::<String, _>("detail")
            .contains("ironflow_events_null_seq_idx")
    }));
    let sequences =
        sqlx::query("SELECT sequence FROM ironflow_events WHERE run_id = ? ORDER BY sequence")
            .bind(run_id)
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.get::<i64, _>("sequence"))
            .collect::<Vec<_>>();
    assert_eq!(sequences.len(), LEGACY_EVENTS);
    assert_eq!(sequences.first(), Some(&1));
    assert_eq!(
        sequences.last(),
        Some(&i64::try_from(LEGACY_EVENTS).unwrap())
    );
    assert!(sequences.windows(2).all(|pair| pair[1] == pair[0] + 1));
    let counter: i64 =
        sqlx::query_scalar("SELECT last_sequence FROM ironflow_event_sequences WHERE run_id = ?")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(counter, i64::try_from(LEGACY_EVENTS).unwrap());
    pool.close().await;

    // Simulate an older process writing after this process completed startup.
    // A read must adopt the row instead of treating its NULL sequence as a
    // corrupt cursor position.
    let late_legacy = ordered_event(run_id, "late-legacy-writer", -20_000);
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(
        "INSERT INTO ironflow_events \
         (id, run_id, event_type, event_json, timestamp) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&late_legacy.id)
    .bind(&late_legacy.run_id)
    .bind(late_legacy.event_type.as_sse_name())
    .bind(serde_json::to_string(&late_legacy).unwrap())
    .bind(late_legacy.timestamp.to_rfc3339())
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
    assert_eq!(
        store
            .list_since(run_id, None, LEGACY_EVENTS + 1)
            .await
            .unwrap()
            .last(),
        Some(&late_legacy)
    );

    // A newly published event remains last even when its wall clock predates
    // every migrated row.
    let after_migration = ordered_event(run_id, "after-migration", -10_000);
    store.publish(after_migration.clone()).await.unwrap();
    assert_eq!(
        store
            .list_since(run_id, None, LEGACY_EVENTS + 2)
            .await
            .unwrap()
            .last(),
        Some(&after_migration)
    );

    drop(store);
    let reopened = SqlEventStore::new(&url).await.unwrap();
    assert_eq!(
        reopened.delete_run(run_id).await.unwrap(),
        LEGACY_EVENTS + 2
    );
    assert_eq!(reopened.delete_run(run_id).await.unwrap(), 0);
    assert_eq!(
        reopened
            .publish(ordered_event(run_id, "after-delete", 50_000))
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );
    assert!(
        reopened
            .list_since(run_id, None, 1)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn sqlite_event_store_serializes_concurrent_publishers_and_retry_ids() {
    const PUBLISHERS: usize = 24;
    const RETRIES: usize = 12;

    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_event_url(dir.path(), "event-concurrency.sqlite");
    let store = Arc::new(SqlEventStore::new(&url).await.unwrap());
    let barrier = Arc::new(tokio::sync::Barrier::new(PUBLISHERS + 1));
    let mut publishers = Vec::with_capacity(PUBLISHERS);
    for index in 0..PUBLISHERS {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        let event = ordered_event("sql-concurrent", &format!("event-{index:02}"), 0);
        publishers.push(tokio::spawn(async move {
            barrier.wait().await;
            store.publish(event).await
        }));
    }
    barrier.wait().await;
    for publisher in publishers {
        publisher.await.unwrap().unwrap();
    }

    let listed = store
        .list_since("sql-concurrent", None, PUBLISHERS + 1)
        .await
        .unwrap();
    assert_eq!(listed.len(), PUBLISHERS);
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let stored = sqlx::query(
        "SELECT id, sequence FROM ironflow_events \
         WHERE run_id = ? ORDER BY sequence",
    )
    .bind("sql-concurrent")
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(stored.len(), PUBLISHERS);
    assert_eq!(
        listed
            .iter()
            .map(|event| event.id.as_str())
            .collect::<Vec<_>>(),
        stored
            .iter()
            .map(|row| row.get::<String, _>("id"))
            .collect::<Vec<_>>()
    );
    assert!(stored.iter().enumerate().all(|(index, row)| {
        row.get::<i64, _>("sequence") == i64::try_from(index + 1).unwrap()
    }));
    pool.close().await;

    let retry = Arc::new(ordered_event("sql-idempotent-race", "same-id", 0));
    let barrier = Arc::new(tokio::sync::Barrier::new(RETRIES + 1));
    let mut retries = Vec::with_capacity(RETRIES);
    for _ in 0..RETRIES {
        let store = Arc::clone(&store);
        let retry = Arc::clone(&retry);
        let barrier = Arc::clone(&barrier);
        retries.push(tokio::spawn(async move {
            barrier.wait().await;
            store.publish((*retry).clone()).await
        }));
    }
    barrier.wait().await;
    for retry in retries {
        retry.await.unwrap().unwrap();
    }
    assert_eq!(
        store
            .list_since("sql-idempotent-race", None, RETRIES)
            .await
            .unwrap(),
        vec![(*retry).clone()]
    );
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let counter: i64 =
        sqlx::query_scalar("SELECT last_sequence FROM ironflow_event_sequences WHERE run_id = ?")
            .bind("sql-idempotent-race")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(counter, 1, "rolled-back retries must not consume positions");
}

#[tokio::test]
async fn sqlite_event_deletion_fences_publish_races_without_orphans() {
    const RACES: usize = 32;

    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_event_url(dir.path(), "event-delete-races.sqlite");
    let store = Arc::new(SqlEventStore::new(&url).await.unwrap());
    for index in 0..RACES {
        let run_id = format!("sql-delete-race-{index:02}");
        store
            .publish(ordered_event(&run_id, "seed", 0))
            .await
            .unwrap();
        let late = ordered_event(&run_id, "late", 1);
        let barrier = Arc::new(tokio::sync::Barrier::new(3));

        let publisher = {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                store.publish(late).await
            })
        };
        let deleter = {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            let run_id = run_id.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                store.delete_run(&run_id).await
            })
        };
        barrier.wait().await;

        let publish_result = publisher.await.unwrap();
        if let Err(error) = publish_result {
            assert_eq!(error.kind(), StorageErrorKind::Conflict);
        }
        let removed = deleter.await.unwrap().unwrap();
        assert!((1..=2).contains(&removed));
        assert!(
            store
                .list_since(&run_id, None, 10)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store
                .publish(ordered_event(&run_id, "after-delete", 2))
                .await
                .unwrap_err()
                .kind(),
            StorageErrorKind::Conflict
        );
    }

    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let tombstones: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_event_deletions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let counters: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_event_sequences")
        .fetch_one(&pool)
        .await
        .unwrap();
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tombstones, i64::try_from(RACES).unwrap());
    assert_eq!(counters, 0);
    assert_eq!(events, 0);
}

#[tokio::test]
async fn sqlite_event_deletion_rejects_suppressed_fence_event_or_sequence_mutations() {
    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_event_url(dir.path(), "event-delete-postconditions.sqlite");
    let store = SqlEventStore::new(&url).await.unwrap();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let cases = [
        (
            "ignore_event_fence",
            "sql-ignored-fence",
            "CREATE TRIGGER ignore_event_fence BEFORE INSERT ON ironflow_event_deletions \
             WHEN NEW.run_id = 'sql-ignored-fence' BEGIN SELECT RAISE(IGNORE); END",
        ),
        (
            "ignore_event_rows",
            "sql-ignored-events",
            "CREATE TRIGGER ignore_event_rows BEFORE DELETE ON ironflow_events \
             WHEN OLD.run_id = 'sql-ignored-events' BEGIN SELECT RAISE(IGNORE); END",
        ),
        (
            "ignore_event_sequence",
            "sql-ignored-sequence",
            "CREATE TRIGGER ignore_event_sequence BEFORE DELETE ON ironflow_event_sequences \
             WHEN OLD.run_id = 'sql-ignored-sequence' BEGIN SELECT RAISE(IGNORE); END",
        ),
    ];

    for (trigger, run_id, create_trigger) in cases {
        let event = ordered_event(run_id, "seed", 0);
        store.publish(event.clone()).await.unwrap();
        sqlx::query(create_trigger).execute(&pool).await.unwrap();

        assert_eq!(
            store.delete_run(run_id).await.unwrap_err().kind(),
            StorageErrorKind::Corruption
        );
        assert_eq!(
            store.list_since(run_id, None, 10).await.unwrap(),
            vec![event]
        );

        let drop_trigger = format!("DROP TRIGGER {trigger}");
        sqlx::query(sqlx::AssertSqlSafe(drop_trigger.as_str()))
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(store.delete_run(run_id).await.unwrap(), 1);
        assert_eq!(
            store
                .publish(ordered_event(run_id, "after-delete", 1))
                .await
                .unwrap_err()
                .kind(),
            StorageErrorKind::Conflict
        );
    }
}

#[tokio::test]
async fn sqlite_event_store_classifies_identity_and_corruption_failures() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("event-errors.sqlite").to_string_lossy()
    );
    let store = SqlEventStore::new(&url).await.unwrap();
    assert_event_identity_contract(&store, "sql-contract").await;

    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query("INSERT INTO ironflow_event_sequences (run_id, last_sequence) VALUES (?, ?)")
        .bind("sql-negative-counter")
        .bind(-2_i64)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .publish(ordered_event("sql-negative-counter", "rejected", 0))
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    let rejected: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_events WHERE run_id = ?")
        .bind("sql-negative-counter")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        rejected, 0,
        "a corrupt counter published an unreadable event"
    );

    let corrupt = RunEvent::run(
        "sql-corrupt",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    store.publish(corrupt.clone()).await.unwrap();
    sqlx::query("UPDATE ironflow_events SET event_json = '{broken' WHERE id = ?")
        .bind(&corrupt.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .list_since("sql-corrupt", Some(&corrupt.id), 10)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );

    sqlx::query("DROP TABLE ironflow_events")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .list_since("sql-backend", None, 10)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Backend
    );
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
        "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' \
         AND name IN ('tenant_a_events', 'tenant_a_event_sequences', 'tenant_a_event_deletions')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>("count"), 3);
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_event_store_obeys_cursor_contract_and_classifies_failures() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_sql_prefix("pg_events");
    let legacy = ordered_event("postgres-legacy-migration", "shared-id", 0);
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let create_legacy = format!(
        "CREATE TABLE {prefix}events (\
         id TEXT NOT NULL, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL, \
         CONSTRAINT {prefix}legacy_pk PRIMARY KEY (id))"
    );
    sqlx::query(sqlx::AssertSqlSafe(create_legacy.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    let insert_legacy = format!(
        "INSERT INTO {prefix}events \
         (id, run_id, event_type, event_json, timestamp) VALUES ($1, $2, $3, $4, $5)"
    );
    sqlx::query(sqlx::AssertSqlSafe(insert_legacy.as_str()))
        .bind(&legacy.id)
        .bind(&legacy.run_id)
        .bind(legacy.event_type.as_sse_name())
        .bind(serde_json::to_string(&legacy).unwrap())
        .bind(legacy.timestamp.to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let store = Arc::new(
        SqlEventStore::new_with_prefix(&url, Some(&prefix))
            .await
            .unwrap(),
    );
    assert_eq!(
        store.list_since(&legacy.run_id, None, 10).await.unwrap(),
        vec![legacy.clone()]
    );
    let mut reused = ordered_event("postgres-migrated-other", "reused", 1);
    reused.id.clone_from(&legacy.id);
    store.publish(reused.clone()).await.unwrap();
    assert_eq!(store.delete_run(&legacy.run_id).await.unwrap(), 1);
    assert_eq!(
        store.list_since(&reused.run_id, None, 10).await.unwrap(),
        vec![reused]
    );
    assert_event_cursor_contract(store.as_ref(), "postgres-cursor").await;

    let barrier = Arc::new(tokio::sync::Barrier::new(13));
    let mut publishers = Vec::new();
    for index in 0..12 {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        let event = ordered_event("postgres-concurrent", &format!("event-{index:02}"), 0);
        publishers.push(tokio::spawn(async move {
            barrier.wait().await;
            store.publish(event).await
        }));
    }
    barrier.wait().await;
    for publisher in publishers {
        publisher.await.unwrap().unwrap();
    }
    assert_eq!(
        store
            .list_since("postgres-concurrent", None, 20)
            .await
            .unwrap()
            .len(),
        12
    );

    let delete_run = "postgres-delete-race";
    store
        .publish(ordered_event(delete_run, "seed", 0))
        .await
        .unwrap();
    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let publisher = {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        tokio::spawn(async move {
            barrier.wait().await;
            store.publish(ordered_event(delete_run, "racing", 1)).await
        })
    };
    let deleter = {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        tokio::spawn(async move {
            barrier.wait().await;
            store.delete_run(delete_run).await
        })
    };
    barrier.wait().await;
    if let Err(error) = publisher.await.unwrap() {
        assert_eq!(error.kind(), StorageErrorKind::Conflict);
    }
    assert!((1..=2).contains(&deleter.await.unwrap().unwrap()));
    assert!(
        store
            .list_since(delete_run, None, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .publish(ordered_event(delete_run, "after-delete", 2))
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );

    let corrupt = RunEvent::run(
        "postgres-corrupt",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    store.publish(corrupt.clone()).await.unwrap();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE {prefix}events SET event_json = '{{broken' WHERE id = $1"
    )))
    .bind(&corrupt.id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        store
            .list_since("postgres-corrupt", Some(&corrupt.id), 10)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );

    cleanup_postgres_event_tables(&url, &prefix).await;
    assert_eq!(
        store
            .list_since("postgres-backend", None, 10)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Backend
    );
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
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TABLE IF EXISTS {}events",
            prefix
        )))
        .execute(&pool)
        .await;
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TABLE IF EXISTS {}event_sequences",
            prefix
        )))
        .execute(&pool)
        .await;
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TABLE IF EXISTS {}event_deletions",
            prefix
        )))
        .execute(&pool)
        .await;
    }
}

#[cfg(feature = "redis")]
#[tokio::test]
async fn redis_event_store_persists_and_lists_events_after_cursor() {
    use redis_support::RedisTest;

    let Some(fixture) = RedisTest::connect("event_store").await else {
        return;
    };
    let store = fixture.event_store(Some(60)).await;
    assert_event_cursor_contract(&store, "redis-cursor").await;

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

    let mut conn = fixture.connection().await.unwrap();
    let base = format!("{}events:run-redis-1", fixture.prefix);
    for key in [
        base.clone(),
        format!("{base}:index"),
        format!("{base}:seq"),
        format!("{base}:meta"),
    ] {
        let ttl: i64 = redis::cmd("TTL")
            .arg(key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!((1..=60).contains(&ttl));
    }

    assert_event_identity_contract(&store, "redis-contract").await;

    let expired_run = "redis-expired-cursor";
    let expired = ordered_event(expired_run, "event", 10);
    store.publish(expired.clone()).await.unwrap();
    let expired_base = format!("{}events:{expired_run}", fixture.prefix);
    let _: usize = redis::cmd("DEL")
        .arg(&[
            expired_base.clone(),
            format!("{expired_base}:index"),
            format!("{expired_base}:seq"),
            format!("{expired_base}:meta"),
        ])
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        store
            .list_since(expired_run, Some(&expired.id), 10)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );

    let corrupt_run = "redis-corrupt-events";
    let corrupt_base = format!("{}events:{corrupt_run}", fixture.prefix);
    let _: () = redis::cmd("SET")
        .arg(corrupt_base)
        .arg("wrong event key type")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        store
            .list_since(corrupt_run, None, 10)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );

    fixture.cleanup().await;
}

#[cfg(feature = "redis")]
#[tokio::test]
async fn redis_event_store_classifies_invalid_backend_configuration() {
    let error = match RedisEventStore::new("not-a-redis-url", None, None).await {
        Ok(_) => panic!("invalid Redis URL unexpectedly created an event store"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), StorageErrorKind::Backend);
}
