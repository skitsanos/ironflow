use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::StorageErrorKind;
use ironflow::storage::event_store::{EventStore, SqlEventStore};
use sqlx::Row;

fn sqlite_url(directory: &std::path::Path, name: &str) -> String {
    format!("sqlite://{}?mode=rwc", directory.join(name).display())
}

fn event(run_id: &str, id: &str) -> RunEvent {
    let mut event = RunEvent::run(
        run_id,
        "migration",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    event.id = id.to_string();
    event
}

async fn create_legacy_events_table(url: &str) -> sqlx::AnyPool {
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect(url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_events (\
         id TEXT PRIMARY KEY, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn insert_legacy_event(pool: &sqlx::AnyPool, event: &RunEvent) {
    sqlx::query(
        "INSERT INTO ironflow_events \
         (id, run_id, event_type, event_json, timestamp) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&event.id)
    .bind(&event.run_id)
    .bind(event.event_type.as_sse_name())
    .bind(serde_json::to_string(event).unwrap())
    .bind(event.timestamp.to_rfc3339())
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_identity_migration_is_serialized_preserving_and_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let url = sqlite_url(directory.path(), "concurrent.sqlite");
    let pool = create_legacy_events_table(&url).await;
    let original = event("legacy-run", "shared-id");
    insert_legacy_event(&pool, &original).await;
    pool.close().await;

    let (first, second) = tokio::join!(SqlEventStore::new(&url), SqlEventStore::new(&url));
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(
        first.list_since("legacy-run", None, 10).await.unwrap(),
        vec![original]
    );
    assert_eq!(
        second.list_since("legacy-run", None, 10).await.unwrap(),
        first.list_since("legacy-run", None, 10).await.unwrap()
    );

    let reused = event("independent-run", "shared-id");
    first.publish(reused.clone()).await.unwrap();
    assert_eq!(first.delete_run("legacy-run").await.unwrap(), 1);
    assert_eq!(
        first.list_since("independent-run", None, 10).await.unwrap(),
        vec![reused]
    );

    drop(first);
    drop(second);
    let reopened = SqlEventStore::new(&url).await.unwrap();
    assert_eq!(
        reopened
            .list_since("independent-run", None, 10)
            .await
            .unwrap()
            .len(),
        1
    );

    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let mut primary_key = sqlx::query("PRAGMA table_info('ironflow_events')")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|row| {
            let position = row.get::<i64, _>("pk");
            (position > 0).then(|| (position, row.get::<String, _>("name")))
        })
        .collect::<Vec<_>>();
    primary_key.sort_by_key(|(position, _)| *position);
    assert_eq!(
        primary_key
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>(),
        ["run_id", "id"]
    );
}

#[tokio::test]
async fn sqlite_identity_migration_rejects_mixed_case_inbound_foreign_keys() {
    let directory = tempfile::tempdir().unwrap();
    let url = sqlite_url(directory.path(), "foreign-key.sqlite");
    let pool = create_legacy_events_table(&url).await;
    sqlx::query(
        "CREATE TABLE event_children (\
         event_id TEXT PRIMARY KEY REFERENCES IRONFLOW_EVENTS(id) ON DELETE CASCADE)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let stored = event("protected-run", "protected-event");
    insert_legacy_event(&pool, &stored).await;
    sqlx::query("INSERT INTO event_children (event_id) VALUES (?)")
        .bind(&stored.id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let error = match SqlEventStore::new(&url).await {
        Ok(_) => panic!("migration unexpectedly removed an inbound foreign key"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), StorageErrorKind::Conflict);
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ironflow_events")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM event_children")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn sqlite_identity_migration_rejects_generated_or_extra_columns() {
    sqlx::any::install_default_drivers();
    let directory = tempfile::tempdir().unwrap();
    let url = sqlite_url(directory.path(), "generated.sqlite");
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_events (\
         id TEXT PRIMARY KEY, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL, \
         derived TEXT AS (id) VIRTUAL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let error = match SqlEventStore::new(&url).await {
        Ok(_) => panic!("migration unexpectedly discarded a generated column"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), StorageErrorKind::Conflict);
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let columns = sqlx::query("PRAGMA table_xinfo('ironflow_events')")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        columns
            .iter()
            .any(|row| row.get::<String, _>("name") == "derived")
    );
}

#[tokio::test]
async fn sqlite_composite_identity_rejects_an_additional_global_unique_index() {
    sqlx::any::install_default_drivers();
    let directory = tempfile::tempdir().unwrap();
    let url = sqlite_url(directory.path(), "global-unique.sqlite");
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_events (\
         id TEXT NOT NULL, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL, sequence BIGINT, \
         PRIMARY KEY (run_id, id), UNIQUE(id))",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let error = match SqlEventStore::new(&url).await {
        Ok(_) => panic!("globally unique event IDs unexpectedly passed validation"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), StorageErrorKind::Conflict);
}

#[tokio::test]
async fn sqlite_composite_identity_rejects_a_nonunique_managed_sequence_index() {
    sqlx::any::install_default_drivers();
    let directory = tempfile::tempdir().unwrap();
    let url = sqlite_url(directory.path(), "spoofed-sequence-index.sqlite");
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_events (\
         id TEXT NOT NULL, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL, sequence BIGINT, \
         PRIMARY KEY (run_id, id))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE INDEX ironflow_events_run_seq_idx \
         ON ironflow_events(run_id, sequence)",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let error = match SqlEventStore::new(&url).await {
        Ok(_) => panic!("non-unique managed sequence index unexpectedly passed validation"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), StorageErrorKind::Conflict);
}

#[tokio::test]
async fn sqlite_legacy_sequence_backfill_rejects_a_no_progress_update() {
    sqlx::any::install_default_drivers();
    let directory = tempfile::tempdir().unwrap();
    let url = sqlite_url(directory.path(), "suppressed-backfill.sqlite");
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_events (\
         id TEXT NOT NULL, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL, sequence BIGINT, \
         PRIMARY KEY (run_id, id))",
    )
    .execute(&pool)
    .await
    .unwrap();
    let stored = event("suppressed-run", "suppressed-event");
    insert_legacy_event(&pool, &stored).await;
    sqlx::query(
        "CREATE TRIGGER suppress_sequence_backfill \
         BEFORE UPDATE OF sequence ON ironflow_events \
         BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let result = tokio::time::timeout(std::time::Duration::from_secs(2), SqlEventStore::new(&url))
        .await
        .expect("a suppressed backfill must fail instead of retrying forever");
    let error = match result {
        Ok(_) => panic!("a suppressed legacy sequence update unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), StorageErrorKind::Corruption);

    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let sequence: Option<i64> =
        sqlx::query_scalar("SELECT sequence FROM ironflow_events WHERE id = ? AND run_id = ?")
            .bind(&stored.id)
            .bind(&stored.run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sequence, None, "failed backfill must roll its batch back");
}

#[tokio::test]
async fn sqlite_legacy_sequence_backfill_rejects_a_no_progress_delete() {
    sqlx::any::install_default_drivers();
    let directory = tempfile::tempdir().unwrap();
    let url = sqlite_url(directory.path(), "suppressed-backfill-delete.sqlite");
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_events (\
         id TEXT NOT NULL, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL, sequence BIGINT, \
         PRIMARY KEY (run_id, id))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE ironflow_event_deletions (run_id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    let stored = event("suppressed-delete-run", "suppressed-delete-event");
    insert_legacy_event(&pool, &stored).await;
    sqlx::query("INSERT INTO ironflow_event_deletions (run_id) VALUES (?)")
        .bind(&stored.run_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER suppress_legacy_event_delete \
         BEFORE DELETE ON ironflow_events \
         BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let result = tokio::time::timeout(std::time::Duration::from_secs(2), SqlEventStore::new(&url))
        .await
        .expect("a suppressed legacy delete must fail instead of retrying forever");
    let error = match result {
        Ok(_) => panic!("a suppressed legacy event deletion unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), StorageErrorKind::Corruption);

    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_events WHERE id = ? AND run_id = ?")
            .bind(&stored.id)
            .bind(&stored.run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1, "failed backfill must roll its batch back");
}

#[tokio::test]
async fn sqlite_legacy_sequence_backfill_rejects_suppressed_counter_cleanup() {
    sqlx::any::install_default_drivers();
    let directory = tempfile::tempdir().unwrap();
    let url = sqlite_url(directory.path(), "suppressed-counter-delete.sqlite");
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_events (\
         id TEXT NOT NULL, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL, sequence BIGINT, \
         PRIMARY KEY (run_id, id))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE ironflow_event_deletions (run_id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_event_sequences (\
         run_id TEXT PRIMARY KEY, last_sequence BIGINT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let stored = event("suppressed-counter-run", "suppressed-counter-event");
    insert_legacy_event(&pool, &stored).await;
    sqlx::query("INSERT INTO ironflow_event_deletions (run_id) VALUES (?)")
        .bind(&stored.run_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER suppress_legacy_counter_delete \
         BEFORE DELETE ON ironflow_event_sequences \
         BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let result = tokio::time::timeout(std::time::Duration::from_secs(2), SqlEventStore::new(&url))
        .await
        .expect("suppressed counter cleanup must fail instead of committing partial cleanup");
    let error = match result {
        Ok(_) => panic!("suppressed legacy counter cleanup unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), StorageErrorKind::Corruption);

    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let event_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_events WHERE id = ? AND run_id = ?")
            .bind(&stored.id)
            .bind(&stored.run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let counter_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_event_sequences WHERE run_id = ?")
            .bind(&stored.run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_count, 1, "failed backfill must roll its batch back");
    assert_eq!(counter_count, 0, "the temporary lock row must roll back");
}

#[cfg(feature = "postgres")]
fn postgres_database_url() -> Option<String> {
    dotenvy::dotenv().ok();
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"))
}

#[cfg(feature = "postgres")]
fn unique_postgres_prefix(label: &str) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("{label}_{}_", &id[..8])
}

#[cfg(feature = "postgres")]
async fn create_postgres_events_table(url: &str, prefix: &str, primary_key: &str) -> sqlx::AnyPool {
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect(url).await.unwrap();
    let sql = format!(
        "CREATE TABLE {prefix}events (\
         id TEXT NOT NULL, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL, sequence BIGINT, \
         PRIMARY KEY ({primary_key}))"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    pool
}

#[cfg(feature = "postgres")]
async fn cleanup_postgres_event_tables(url: &str, prefix: &str) {
    let Ok(pool) = sqlx::AnyPool::connect(url).await else {
        return;
    };
    for table in ["events", "event_sequences", "event_deletions"] {
        let sql = format!("DROP TABLE IF EXISTS {prefix}{table}");
        let _ = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&pool)
            .await;
    }
}

#[cfg(feature = "postgres")]
async fn expect_postgres_identity_conflict(url: &str, prefix: &str) {
    let error = match SqlEventStore::new_with_prefix(url, Some(prefix)).await {
        Ok(_) => panic!("unsupported PostgreSQL identity constraint passed validation"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), StorageErrorKind::Conflict);
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_identity_migration_rejects_an_independent_global_unique_constraint() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_postgres_prefix("pg_identity_unique");
    let pool = create_postgres_events_table(&url, &prefix, "id").await;
    let constraint_sql = format!(
        "ALTER TABLE {prefix}events ADD CONSTRAINT {prefix}external_global_id_unique UNIQUE(id)"
    );
    sqlx::query(sqlx::AssertSqlSafe(constraint_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    let insert_sql = format!(
        "INSERT INTO {prefix}events \
         (id, run_id, event_type, event_json, timestamp) \
         VALUES ('shared-id', 'legacy-run', 'run_started', '{{}}', '2026-01-01T00:00:00Z')"
    );
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    expect_postgres_identity_conflict(&url, &prefix).await;
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let primary_key: Vec<String> = sqlx::query_scalar(
        "SELECT attribute.attname::text \
         FROM pg_catalog.pg_constraint AS constraint_row \
         JOIN LATERAL unnest(constraint_row.conkey) WITH ORDINALITY \
              AS key_column(attnum, ordinality) ON true \
         JOIN pg_catalog.pg_attribute AS attribute \
           ON attribute.attrelid = constraint_row.conrelid \
          AND attribute.attnum = key_column.attnum \
         WHERE constraint_row.conrelid = to_regclass($1) \
           AND constraint_row.contype = 'p' \
         ORDER BY key_column.ordinality",
    )
    .bind(format!("{prefix}events"))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(primary_key, ["id"]);
    let count_sql = format!("SELECT COUNT(*) FROM {prefix}events");
    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    pool.close().await;
    cleanup_postgres_event_tables(&url, &prefix).await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_composite_identity_rejects_a_spoofed_managed_unique_index() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_postgres_prefix("pg_identity_spoofed");
    let pool = create_postgres_events_table(&url, &prefix, "run_id, id").await;
    let index_sql = format!("CREATE UNIQUE INDEX {prefix}events_run_seq_idx ON {prefix}events(id)");
    sqlx::query(sqlx::AssertSqlSafe(index_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    expect_postgres_identity_conflict(&url, &prefix).await;
    cleanup_postgres_event_tables(&url, &prefix).await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_composite_identity_rejects_a_nonunique_managed_sequence_index() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_postgres_prefix("pg_identity_nonunique");
    let pool = create_postgres_events_table(&url, &prefix, "run_id, id").await;
    let index_sql =
        format!("CREATE INDEX {prefix}events_run_seq_idx ON {prefix}events(run_id, sequence)");
    sqlx::query(sqlx::AssertSqlSafe(index_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    expect_postgres_identity_conflict(&url, &prefix).await;
    cleanup_postgres_event_tables(&url, &prefix).await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_composite_identity_rejects_a_deferrable_primary_key() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_postgres_prefix("pg_identity_deferrable");
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let create_sql = format!(
        "CREATE TABLE {prefix}events (\
         id TEXT NOT NULL, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
         event_json TEXT NOT NULL, timestamp TEXT NOT NULL, sequence BIGINT, \
         PRIMARY KEY (run_id, id) DEFERRABLE INITIALLY IMMEDIATE)"
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    expect_postgres_identity_conflict(&url, &prefix).await;
    cleanup_postgres_event_tables(&url, &prefix).await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_composite_identity_rejects_a_global_exclusion_constraint() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_postgres_prefix("pg_identity_exclusion");
    let pool = create_postgres_events_table(&url, &prefix, "run_id, id").await;
    let constraint_sql = format!(
        "ALTER TABLE {prefix}events ADD CONSTRAINT {prefix}global_id_exclusion \
         EXCLUDE USING gist (int4range(hashtext(id), hashtext(id), '[]') WITH &&)"
    );
    sqlx::query(sqlx::AssertSqlSafe(constraint_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    expect_postgres_identity_conflict(&url, &prefix).await;
    cleanup_postgres_event_tables(&url, &prefix).await;
}
