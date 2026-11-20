//! PostgreSQL startup and delete locking (IF-202).
//!
//! A starting replica must not wait behind, or queue live replicas behind,
//! sessions that merely use the tables; the one-time identity migration must
//! be bounded; the counter repair must leave current rows alone; and a delete
//! must not remove a run whose lease appeared while the delete was running.
//!
//! Live tests: gated on a PostgreSQL `DATABASE_URL`, and mandatory when
//! `IRONFLOW_POSTGRES_TEST_REQUIRED` is set. Every bound below is a generous
//! ceiling for a loaded machine, not an expected duration.

#![cfg(feature = "postgres")]

use std::time::Duration;

use ironflow::engine::types::{Context, RunStatus};
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::event_store::{EventStore, SqlEventStore};
use ironflow::storage::sql_store::SqlStateStore;
use ironflow::storage::{RunLease, StateStore, StorageErrorKind};
use sqlx::{AnyPool, Row};

/// Ceiling for an operation that must not wait for a lock at all.
const UNBLOCKED: Duration = Duration::from_secs(30);
/// Ceiling for the bounded migration: its lock timeout plus generous slack.
const BOUNDED: Duration = Duration::from_secs(60);

struct Database {
    url: String,
    prefix: String,
    admin: AnyPool,
}

impl Database {
    async fn from_env(label: &str) -> Option<Self> {
        dotenvy::dotenv().ok();
        let required = std::env::var("IRONFLOW_POSTGRES_TEST_REQUIRED")
            .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"));
        let url = std::env::var("DATABASE_URL")
            .ok()
            .filter(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"));
        let Some(url) = url else {
            assert!(
                !required,
                "PostgreSQL integration tests require a PostgreSQL DATABASE_URL"
            );
            eprintln!("Skipping PostgreSQL test: DATABASE_URL is not configured for PostgreSQL");
            return None;
        };
        sqlx::any::install_default_drivers();
        let admin = sqlx::any::AnyPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .expect("connect the PostgreSQL test session pool");
        let id = uuid::Uuid::new_v4().simple().to_string();
        Some(Self {
            url,
            prefix: format!("{label}_{}_", &id[..8]),
            admin,
        })
    }

    fn table(&self, suffix: &str) -> String {
        format!("{}{suffix}", self.prefix)
    }

    async fn event_store(&self) -> ironflow::storage::StorageResult<SqlEventStore> {
        SqlEventStore::new_with_prefix(&self.url, Some(&self.prefix)).await
    }

    async fn state_store(&self) -> ironflow::storage::StorageResult<SqlStateStore> {
        SqlStateStore::new_with_prefix(&self.url, Some(&self.prefix)).await
    }

    async fn execute(&self, sql: String) {
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&self.admin)
            .await
            .unwrap_or_else(|error| panic!("{sql}: {error}"));
    }

    /// Open a transaction that holds `mode` on every named table until the
    /// returned guard is rolled back, as a dump, a long query or an idle
    /// transaction would.
    async fn hold_locks(
        &self,
        suffixes: &[&str],
        mode: &str,
    ) -> sqlx::Transaction<'static, sqlx::Any> {
        let mut session = self.admin.begin().await.expect("open the locking session");
        for suffix in suffixes {
            let sql = format!("LOCK TABLE {} IN {mode} MODE", self.table(suffix));
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .execute(&mut *session)
                .await
                .unwrap_or_else(|error| panic!("{sql}: {error}"));
        }
        session
    }

    async fn primary_key(&self, suffix: &str) -> Vec<String> {
        sqlx::query(
            "SELECT attribute.attname::text AS column_name \
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
        .bind(self.table(suffix))
        .fetch_all(&self.admin)
        .await
        .expect("read the primary key")
        .iter()
        .map(|row| row.get::<String, _>("column_name"))
        .collect()
    }

    async fn cleanup(self) {
        for suffix in [
            "events",
            "event_sequences",
            "event_deletions",
            "tasks",
            "run_leases",
            "schedule_claims",
            "runs",
        ] {
            let sql = format!("DROP TABLE IF EXISTS {} CASCADE", self.table(suffix));
            let _ = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .execute(&self.admin)
                .await;
        }
        let sql = format!("DROP FUNCTION IF EXISTS {}()", self.table("pause"));
        let _ = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&self.admin)
            .await;
        self.admin.close().await;
    }
}

fn event(run_id: &str, id: &str) -> RunEvent {
    let mut event = RunEvent::run(
        run_id,
        "startup-locks",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    event.id = id.to_string();
    event
}

const EVENT_TABLES: [&str; 3] = ["events", "event_sequences", "event_deletions"];
const STATE_TABLES: [&str; 4] = ["runs", "tasks", "run_leases", "schedule_claims"];

/// A reader (dump, analytics query, idle transaction) holds ACCESS SHARE.
/// Before the fix startup requested ACCESS EXCLUSIVE and waited for it.
#[tokio::test]
async fn event_store_startup_completes_while_a_reader_holds_the_events_table() {
    let Some(database) = Database::from_env("if202_reader").await else {
        return;
    };
    let first = database.event_store().await.expect("first replica starts");
    first.publish(event("run", "event-1")).await.unwrap();

    let reader = database.hold_locks(&EVENT_TABLES, "ACCESS SHARE").await;
    let second = tokio::time::timeout(UNBLOCKED, database.event_store()).await;
    reader.rollback().await.unwrap();

    let outcome = second.map(|store| store.map(|_| ()));
    database.cleanup().await;
    outcome
        .expect("startup waited for a lock while a reader held the events table")
        .expect("startup failed while a reader held the events table");
}

/// The pending exclusive request of a starting replica queued every new
/// statement of the live replicas behind it.
#[tokio::test]
async fn publishing_and_reading_continue_while_a_second_replica_starts() {
    let Some(database) = Database::from_env("if202_live").await else {
        return;
    };
    let live = database.event_store().await.expect("live replica starts");
    live.publish(event("run", "event-1")).await.unwrap();

    let reader = database.hold_locks(&EVENT_TABLES, "ACCESS SHARE").await;
    let url = database.url.clone();
    let prefix = database.prefix.clone();
    let starting =
        tokio::spawn(async move { SqlEventStore::new_with_prefix(&url, Some(&prefix)).await });
    // Give the starting replica time to either finish or reach a lock wait.
    let deadline = tokio::time::Instant::now() + UNBLOCKED;
    while !starting.is_finished() && tokio::time::Instant::now() < deadline {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
             WHERE datname = current_database() AND pid <> pg_backend_pid() \
             AND wait_event_type = 'Lock' AND strpos(query, $1) > 0)",
        )
        .bind(database.table("events"))
        .fetch_one(&database.admin)
        .await
        .unwrap();
        if waiting {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let published = tokio::time::timeout(UNBLOCKED, live.publish(event("run", "event-2"))).await;
    let listed = tokio::time::timeout(UNBLOCKED, live.list_since("run", None, 10)).await;
    reader.rollback().await.unwrap();
    let started = tokio::time::timeout(UNBLOCKED, starting).await;

    database.cleanup().await;
    published
        .expect("a live publisher stalled behind the starting replica")
        .expect("publish failed during a second replica's startup");
    let listed = listed
        .expect("a live reader stalled behind the starting replica")
        .expect("list failed during a second replica's startup");
    assert_eq!(listed.len(), 2);
    started
        .expect("the second replica did not finish starting")
        .expect("startup task panicked")
        .expect("the second replica failed to start");
}

/// An open writer transaction holds ROW EXCLUSIVE. `CREATE INDEX IF NOT
/// EXISTS` requests SHARE before it notices the index exists, so an
/// unconditional create waits for every open writer.
#[tokio::test]
async fn startup_does_not_wait_for_an_open_writer_transaction() {
    let Some(database) = Database::from_env("if202_writer").await else {
        return;
    };
    database.state_store().await.expect("first state store");
    database.event_store().await.expect("first event store");

    let mut tables = EVENT_TABLES.to_vec();
    tables.extend(STATE_TABLES);
    let writer = database.hold_locks(&tables, "ROW EXCLUSIVE").await;
    let events = tokio::time::timeout(UNBLOCKED, database.event_store()).await;
    let state = tokio::time::timeout(UNBLOCKED, database.state_store()).await;
    writer.rollback().await.unwrap();

    let events = events.map(|store| store.map(|_| ()));
    let state = state.map(|store| store.map(|_| ()));
    database.cleanup().await;
    events
        .expect("event store startup waited for an open writer transaction")
        .expect("event store startup failed beside an open writer transaction");
    state
        .expect("state store startup waited for an open writer transaction")
        .expect("state store startup failed beside an open writer transaction");
}

/// The legacy `(id)` key still needs ACCESS EXCLUSIVE. The wait is bounded,
/// fails with a clear error that changes nothing, and a later start migrates.
#[tokio::test]
async fn legacy_identity_migration_is_bounded_by_the_lock_timeout_and_retryable() {
    let Some(database) = Database::from_env("if202_legacy").await else {
        return;
    };
    database
        .execute(format!(
            "CREATE TABLE {} (\
             id TEXT PRIMARY KEY, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
             event_json TEXT NOT NULL, timestamp TEXT NOT NULL, sequence BIGINT)",
            database.table("events")
        ))
        .await;
    let legacy = event("legacy-run", "legacy-event");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "INSERT INTO {} (id, run_id, event_type, event_json, timestamp) \
             VALUES ($1, $2, $3, $4, $5)",
            database.table("events")
        )
        .as_str(),
    ))
    .bind(&legacy.id)
    .bind(&legacy.run_id)
    .bind(legacy.event_type.as_sse_name())
    .bind(serde_json::to_string(&legacy).unwrap())
    .bind(ironflow::util::timestamp::rfc3339_offset(legacy.timestamp))
    .execute(&database.admin)
    .await
    .unwrap();

    let reader = database.hold_locks(&["events"], "ACCESS SHARE").await;
    let blocked = tokio::time::timeout(BOUNDED, database.event_store()).await;
    let key_while_blocked = database.primary_key("events").await;
    reader.rollback().await.unwrap();
    let blocked = blocked.map(|store| store.map(|_| ()));

    let retried = tokio::time::timeout(BOUNDED, database.event_store()).await;
    let key_after = database.primary_key("events").await;
    let events = match &retried {
        Ok(Ok(store)) => Some(store.list_since("legacy-run", None, 10).await),
        _ => None,
    };
    let retried = retried.map(|store| store.map(|_| ()));
    database.cleanup().await;

    let error = blocked
        .expect("the legacy migration waited for its lock without a bound")
        .expect_err("the legacy migration cannot succeed while a reader holds the table");
    assert_eq!(error.kind(), StorageErrorKind::Conflict, "{error}");
    let message = error.to_string();
    assert!(
        message.contains("lock") && message.contains("identity migration"),
        "the startup error must name the lock and the migration: {message}"
    );
    assert_eq!(
        key_while_blocked,
        ["id"],
        "a timed-out migration changed the key"
    );
    retried
        .expect("the retried migration did not finish")
        .expect("the retried migration failed");
    assert_eq!(key_after, ["run_id", "id"]);
    let events = events.expect("retried store").expect("list legacy events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, "legacy-event");
}

/// A table from a release without the `sequence` column needs `ALTER TABLE`,
/// which also takes ACCESS EXCLUSIVE and is bounded the same way.
#[tokio::test]
async fn column_addition_is_bounded_by_the_lock_timeout_and_retryable() {
    let Some(database) = Database::from_env("if202_column").await else {
        return;
    };
    database
        .execute(format!(
            "CREATE TABLE {} (\
             id TEXT NOT NULL, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
             event_json TEXT NOT NULL, timestamp TEXT NOT NULL, PRIMARY KEY (run_id, id))",
            database.table("events")
        ))
        .await;

    let reader = database.hold_locks(&["events"], "ACCESS SHARE").await;
    let blocked = tokio::time::timeout(BOUNDED, database.event_store()).await;
    reader.rollback().await.unwrap();
    let blocked = blocked.map(|store| store.map(|_| ()));
    let retried = tokio::time::timeout(BOUNDED, database.event_store()).await;
    let retried = retried.map(|store| store.map(|_| ()));
    database.cleanup().await;

    let error = blocked
        .expect("the column addition waited for its lock without a bound")
        .expect_err("the column cannot be added while a reader holds the table");
    assert_eq!(error.kind(), StorageErrorKind::Conflict, "{error}");
    let message = error.to_string();
    assert!(
        message.contains("lock") && message.contains("column 'sequence'"),
        "the startup error must name the lock and the change: {message}"
    );
    retried
        .expect("the retried start did not finish")
        .expect("the retried start failed");
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct CounterRow {
    last_sequence: i64,
    /// Physical row version: `xmin`, `ctid` and `xmax` change when the row is
    /// rewritten or merely locked.
    version: String,
}

async fn counter_rows(database: &Database) -> std::collections::BTreeMap<String, CounterRow> {
    let sql = format!(
        "SELECT run_id, last_sequence, \
         xmin::text || '/' || ctid::text || '/' || xmax::text AS version \
         FROM {}",
        database.table("event_sequences")
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .fetch_all(&database.admin)
        .await
        .expect("read the counter rows")
        .iter()
        .map(|row| {
            (
                row.get::<String, _>("run_id"),
                CounterRow {
                    last_sequence: row.get("last_sequence"),
                    version: row.get("version"),
                },
            )
        })
        .collect()
}

/// Before the fix every counter row was rewritten at every startup.
#[tokio::test]
async fn counter_repair_touches_only_missing_or_stale_rows() {
    let Some(database) = Database::from_env("if202_repair").await else {
        return;
    };
    let store = database.event_store().await.expect("first replica starts");
    for run in ["current-a", "current-b", "missing", "behind", "ahead"] {
        for index in 1..=3 {
            store
                .publish(event(run, &format!("{run}-{index}")))
                .await
                .unwrap();
        }
    }
    let counters = database.table("event_sequences");
    database
        .execute(format!("DELETE FROM {counters} WHERE run_id = 'missing'"))
        .await;
    database
        .execute(format!(
            "UPDATE {counters} SET last_sequence = 1 WHERE run_id = 'behind'"
        ))
        .await;
    database
        .execute(format!(
            "UPDATE {counters} SET last_sequence = 9 WHERE run_id = 'ahead'"
        ))
        .await;
    let before = counter_rows(&database).await;

    let restarted = database.event_store().await.map(|_| ());
    let after = counter_rows(&database).await;
    database.cleanup().await;
    restarted.expect("second replica starts");

    assert_eq!(
        after["missing"].last_sequence, 3,
        "missing counter restored"
    );
    assert_eq!(after["behind"].last_sequence, 3, "stale counter advanced");
    assert_eq!(
        after["ahead"].last_sequence, 9,
        "a counter is never lowered"
    );
    for run in ["current-a", "current-b", "ahead"] {
        assert_eq!(
            after[run], before[run],
            "counter row '{run}' was rewritten or locked although it was current"
        );
    }
}

/// Pause the delete between its lease read and its run-row lock, re-create
/// the run with a live lease through the public API, and let the delete
/// continue. Before the fix it removed the leased run.
#[tokio::test]
async fn delete_racing_a_recreate_with_the_same_id_never_removes_a_leased_run() {
    let Some(database) = Database::from_env("if202_delete").await else {
        return;
    };
    let store = std::sync::Arc::new(database.state_store().await.expect("state store"));
    let lock_key = i64::from(uuid::Uuid::new_v4().as_fields().0);
    let pause = database.table("pause");
    database
        .execute(format!(
            "CREATE FUNCTION {pause}() RETURNS trigger LANGUAGE plpgsql AS $ironflow$ \
             BEGIN \
                 PERFORM pg_advisory_lock({lock_key}); \
                 PERFORM pg_advisory_unlock({lock_key}); \
                 RETURN NULL; \
             END; \
             $ironflow$"
        ))
        .await;
    // A statement-level trigger also fires when the lease update matched no
    // row, which is the state the race needs.
    database
        .execute(format!(
            "CREATE TRIGGER {pause} AFTER UPDATE ON {} \
             FOR EACH STATEMENT EXECUTE FUNCTION {pause}()",
            database.table("run_leases")
        ))
        .await;

    let mut gate = database.admin.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(lock_key)
        .execute(&mut *gate)
        .await
        .unwrap();

    let deleting = {
        let store = store.clone();
        tokio::spawn(async move { store.delete_run("recreated").await })
    };
    let deadline = tokio::time::Instant::now() + UNBLOCKED;
    loop {
        let paused: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
             WHERE datname = current_database() AND pid <> pg_backend_pid() \
             AND wait_event_type = 'Lock' AND wait_event = 'advisory' \
             AND strpos(query, $1) > 0)",
        )
        .bind(database.table("run_leases"))
        .fetch_one(&database.admin)
        .await
        .unwrap();
        if paused {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the delete never reached its lease read"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let lease = RunLease::fresh();
    let created = store
        .init_run_owned("recreated", "flow", &Context::new(), &lease)
        .await;
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(lock_key)
        .execute(&mut *gate)
        .await
        .unwrap();
    drop(gate);
    let deleted = tokio::time::timeout(UNBLOCKED, deleting).await;
    let survivor = store.get_run_info("recreated").await;
    let renewed = store.renew_run_lease("recreated", &lease).await;
    database.cleanup().await;

    created.expect("the run is created while the delete is paused");
    let error = deleted
        .expect("the delete did not finish")
        .expect("delete task panicked")
        .expect_err("the delete removed a run whose owner holds a live lease");
    assert_eq!(error.kind(), StorageErrorKind::Conflict, "{error}");
    survivor.expect("the leased run must survive the delete");
    assert!(
        renewed.expect("renew the surviving lease"),
        "the owner lost its lease to the delete"
    );
}

/// IF-220 (y): a second replica that starts while another one is still
/// building a missing index. The open transaction below is that build: it
/// holds the same locks for as long as the test needs, where a real build on
/// a large table holds them for its duration. The second start is bounded by
/// the lock timeout, fails with an error that names the index, changes
/// nothing, and succeeds once the build has committed.
#[tokio::test]
async fn a_second_replica_starting_during_an_index_build_is_bounded_and_retryable() {
    let Some(database) = Database::from_env("if220_index").await else {
        return;
    };
    drop(database.event_store().await.expect("create the schema"));
    let index = database.table("events_run_seq_idx");
    database.execute(format!("DROP INDEX {index}")).await;

    let mut build = database
        .admin
        .begin()
        .await
        .expect("open the build session");
    let create = format!(
        "CREATE UNIQUE INDEX {index} ON {}(run_id, sequence)",
        database.table("events")
    );
    sqlx::query(sqlx::AssertSqlSafe(create.as_str()))
        .execute(&mut *build)
        .await
        .expect("start the index build");

    let during = tokio::time::timeout(BOUNDED, database.event_store()).await;
    let during = during.map(|store| store.map(|_| ()));
    build.commit().await.expect("finish the index build");
    let after = tokio::time::timeout(BOUNDED, database.event_store()).await;
    let usable = match after {
        Ok(Ok(store)) => store
            .publish(event("if220-run", "if220-event"))
            .await
            .map_err(|error| error.to_string()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("the start after the build did not finish".to_string()),
    };
    database.cleanup().await;

    let error = during
        .expect("the start waited for the index build without a bound")
        .expect_err("a start during the build cannot create the same index");
    assert_eq!(error.kind(), StorageErrorKind::Conflict, "{error}");
    let message = error.to_string();
    assert!(
        message.contains("lock") && message.contains("event sequence index"),
        "the startup error must name the lock and the index: {message}"
    );
    usable.expect("the start after the build must succeed and publish");
}

/// IF-220 (y): the in-place schedule-claim upgrade of IF-201 on PostgreSQL.
/// A claims table from before IF-201 has no `run_micros` column; two replicas
/// of the new binary start together, both must succeed, and the claim an
/// older binary wrote reads as an orphan until its run is recorded.
#[tokio::test]
async fn postgres_claims_table_from_an_older_binary_is_upgraded_in_place() {
    const KEY: &str = "UTC@2026-05-01T02:00";
    let Some(database) = Database::from_env("if220_claims").await else {
        return;
    };
    let claims = database.table("schedule_claims");
    database
        .execute(format!(
            "CREATE TABLE {claims} (\
             name TEXT NOT NULL, claim_key TEXT NOT NULL, claimed_micros BIGINT NOT NULL, \
             PRIMARY KEY (name, claim_key))"
        ))
        .await;
    database
        .execute(format!(
            "INSERT INTO {claims} (name, claim_key, claimed_micros) \
             VALUES ('nightly', '{KEY}', {})",
            ironflow::util::timestamp::unix_micros(jiff::Timestamp::now())
        ))
        .await;

    let result: Result<(), String> = async {
        let (first, second) = tokio::join!(database.state_store(), database.state_store());
        let store = first.map_err(|error| format!("first replica: {error}"))?;
        let peer = second.map_err(|error| format!("second replica: {error}"))?;
        let check =
            |label: &str, condition: bool| condition.then_some(()).ok_or_else(|| label.to_string());
        let error = |error: ironflow::storage::StorageError| error.to_string();

        let claimed = store.claim_schedule("nightly", KEY, 3_600).await;
        check("the old claim was claimed again", !claimed.map_err(error)?)?;
        let recorded = store.schedule_run_recorded("nightly", KEY).await;
        check("the old claim read as recorded", !recorded.map_err(error)?)?;
        store
            .record_schedule_run("nightly", KEY, 3_600)
            .await
            .map_err(error)?;
        let seen = peer.schedule_run_recorded("nightly", KEY).await;
        check("the peer did not see the record", seen.map_err(error)?)?;
        Ok(())
    }
    .await;
    let columns: Vec<String> = sqlx::query(
        "SELECT column_name::text AS column_name FROM information_schema.columns \
         WHERE table_name = $1 ORDER BY ordinal_position",
    )
    .bind(&claims)
    .fetch_all(&database.admin)
    .await
    .expect("read the claim columns")
    .iter()
    .map(|row| row.get::<String, _>("column_name"))
    .collect();
    database.cleanup().await;

    result.unwrap();
    assert_eq!(
        columns,
        ["name", "claim_key", "claimed_micros", "run_micros"],
        "the column must be added exactly once"
    );
}
