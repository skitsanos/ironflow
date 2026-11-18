//! Golden tests for the timestamps the state and event stores persist
//! (IF-231 stage 3).
//!
//! Committed against the `chrono` implementation first. The JSON store
//! records under `storage/fixtures/timestamps/json_store` were written by
//! that implementation; the SQL rows are inserted with the text and integer
//! values recorded from it in `instants.tsv`.

use std::collections::HashMap;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ironflow::engine::events::{RunEvent, RunEventType};
use ironflow::engine::types::*;
use ironflow::storage::event_store::{EventStore, SqlEventStore};
use ironflow::storage::json_store::JsonStateStore;
use ironflow::storage::sql_store::SqlStateStore;
use ironflow::storage::{PageSize, RunCursor, RunListQuery, StateStore};
use sqlx::Row as _;

#[path = "support/timestamps.rs"]
mod timestamps;

#[cfg(feature = "postgres")]
#[path = "support/postgres.rs"]
mod postgres_support;

use timestamps::{Instant, Row, instant, rows};

// ===== JSON store =====

const JSON_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/storage/fixtures/timestamps/json_store"
);

/// `(run id, recorded started text, instant)`, newest first.
fn json_fixture_runs() -> [(&'static str, &'static str, Instant); 5] {
    [
        (
            "golden-nanos",
            "2026-01-02T03:04:05.123456789Z",
            instant(1_767_323_045, 123_456_789),
        ),
        (
            "golden-millis",
            "2026-01-02T03:04:05.120Z",
            instant(1_767_323_045, 120_000_000),
        ),
        (
            "golden-micros",
            "2026-01-02T03:04:05.000100Z",
            instant(1_767_323_045, 100_000),
        ),
        (
            "golden-whole",
            "2026-01-02T03:04:05Z",
            instant(1_767_323_045, 0),
        ),
        (
            "golden-before-epoch",
            "1969-12-31T23:59:58.999999001Z",
            instant(-2, 999_999_001),
        ),
    ]
}

fn copy_json_fixture(target: &Path) {
    for entry in std::fs::read_dir(JSON_FIXTURE).unwrap() {
        let entry = entry.unwrap();
        let copy = target.join(entry.file_name());
        std::fs::copy(entry.path(), &copy).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
}

async fn all_pages(store: &dyn StateStore, status: Option<RunStatus>, size: usize) -> Vec<String> {
    let mut ids = Vec::new();
    let mut after = None;
    loop {
        let query = RunListQuery::new(status.clone(), after, PageSize::new(size).unwrap()).unwrap();
        let page = store.list_run_summaries_page(&query).await.unwrap();
        ids.extend(page.items.iter().map(|summary| summary.id.clone()));
        match page.next {
            Some(cursor) => {
                // Continue through the encoded form, as an API client does.
                after = Some(RunCursor::decode(&cursor.encode().unwrap()).unwrap());
            }
            None => return ids,
        }
    }
}

#[tokio::test]
async fn json_store_loads_records_written_before_the_migration() {
    let dir = tempfile::tempdir().unwrap();
    copy_json_fixture(dir.path());
    let store = JsonStateStore::new(dir.path());

    for (id, _, started) in json_fixture_runs() {
        let info = store.get_run_info(id).await.unwrap();
        assert_eq!(info.started, Some(started), "{id}");
        assert_eq!(info.finished, None, "{id}");
        let task = &info.tasks["step"];
        assert_eq!(task.started, Some(instant(1_767_323_045, 500_000_000)));
        assert_eq!(task.finished, Some(instant(1_767_323_046, 1)));
    }

    let expected: Vec<String> = json_fixture_runs()
        .iter()
        .map(|(id, _, _)| id.to_string())
        .collect();
    assert_eq!(all_pages(&store, None, 2).await, expected);
    assert_eq!(
        all_pages(&store, Some(RunStatus::Running), 1).await,
        expected
    );
}

#[tokio::test]
async fn json_store_rewrites_a_record_with_the_same_timestamp_bytes() {
    let dir = tempfile::tempdir().unwrap();
    copy_json_fixture(dir.path());
    let store = JsonStateStore::new(dir.path());

    for (id, text, _) in json_fixture_runs() {
        let mut task = TaskState::new("later", "log");
        task.started = Some(instant(1_767_323_050, 0));
        store.upsert_task(id, &task).await.unwrap();

        let run = std::fs::read_to_string(dir.path().join(format!("{id}.json"))).unwrap();
        assert!(run.contains(&format!(r#""started": "{text}""#)), "{run}");
        assert!(run.contains(r#""started": "2026-01-02T03:04:05.500Z""#));
        assert!(run.contains(r#""finished": "2026-01-02T03:04:06.000000001Z""#));
        assert!(run.contains(r#""started": "2026-01-02T03:04:10Z""#));
        let summary =
            std::fs::read_to_string(dir.path().join(format!("{id}.summary.json"))).unwrap();
        assert!(
            summary.contains(&format!(r#""started":"{text}","finished":null"#)),
            "{summary}"
        );
    }
}

#[tokio::test]
async fn run_cursor_keeps_its_encoded_bytes() {
    let dir = tempfile::tempdir().unwrap();
    copy_json_fixture(dir.path());
    let store = JsonStateStore::new(dir.path());

    let query = RunListQuery::new(None, None, PageSize::new(2).unwrap()).unwrap();
    let page = store.list_run_summaries_page(&query).await.unwrap();
    let encoded = page.next.unwrap().encode().unwrap();
    assert_eq!(
        String::from_utf8(URL_SAFE_NO_PAD.decode(&encoded).unwrap()).unwrap(),
        r#"{"version":1,"status":null,"started":"2026-01-02T03:04:05.120Z","id":"golden-millis"}"#
    );

    // A cursor issued by a replica that spelled the offset differently.
    let foreign = URL_SAFE_NO_PAD.encode(
        r#"{"version":1,"status":null,"started":"2026-01-02T03:04:05.120+00:00","id":"golden-millis"}"#,
    );
    let query = RunListQuery::new(
        None,
        Some(RunCursor::decode(&foreign).unwrap()),
        PageSize::new(5).unwrap(),
    )
    .unwrap();
    let rest = store.list_run_summaries_page(&query).await.unwrap();
    assert_eq!(
        rest.items
            .iter()
            .map(|run| run.id.as_str())
            .collect::<Vec<_>>(),
        ["golden-micros", "golden-whole", "golden-before-epoch"]
    );
}

// ===== SQL state store =====

struct SqlTarget {
    url: String,
    runs: String,
    tasks: String,
    prefix: Option<String>,
}

impl SqlTarget {
    async fn store(&self) -> SqlStateStore {
        SqlStateStore::new_with_prefix(&self.url, self.prefix.as_deref())
            .await
            .unwrap()
    }

    async fn pool(&self) -> sqlx::AnyPool {
        sqlx::any::install_default_drivers();
        sqlx::AnyPool::connect(&self.url).await.unwrap()
    }
}

async fn execute(pool: &sqlx::AnyPool, sql: String) {
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .execute(pool)
        .await
        .unwrap();
}

fn run_id(row: &Row) -> String {
    format!("row-{}", row.label.replace('_', "-"))
}

/// Newest first, as every store lists: microseconds, then run ID, descending.
fn listing_order(mut runs: Vec<(i64, String)>) -> Vec<String> {
    runs.sort_by(|left, right| right.cmp(left));
    runs.into_iter().map(|(_, id)| id).collect()
}

async fn assert_sql_state_store_forms(target: SqlTarget) {
    let store = target.store().await;
    let pool = target.pool().await;
    let rows = rows();

    // Writer: the task columns carry the recorded nine-digit form.
    store
        .init_run("golden", "f", &HashMap::new())
        .await
        .unwrap();
    store
        .init_run("golden-negative", "f", &HashMap::new())
        .await
        .unwrap();
    for (index, row) in rows.iter().enumerate() {
        let mut task = TaskState::new(&format!("t{index}"), "log");
        task.started = Some(row.instant);
        task.finished = Some(row.instant);
        let run = if row.negative_year() {
            "golden-negative"
        } else {
            "golden"
        };
        store.upsert_task(run, &task).await.unwrap();
    }
    let written = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT name, started, finished FROM {}",
        target.tasks
    )))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(written.len(), rows.len());
    for task in written {
        let name: String = task.get("name");
        let row = &rows[name[1..].parse::<usize>().unwrap()];
        assert_eq!(task.get::<String, _>("started"), row.sql, "{}", row.label);
        assert_eq!(task.get::<String, _>("finished"), row.sql, "{}", row.label);
    }
    let read = store.get_run_info("golden").await.unwrap();
    for (index, row) in rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.negative_year())
    {
        let task = &read.tasks[&format!("t{index}")];
        assert_eq!(task.started, Some(row.instant), "{}", row.label);
        assert_eq!(task.finished, Some(row.instant), "{}", row.label);
    }
    // Accepted difference: the old reader rejected the signed year its own
    // writer produced, so such a run could not be loaded. It loads now.
    let negative = store.get_run_info("golden-negative").await.unwrap();
    assert_eq!(negative.tasks.len(), 2);
    for (index, row) in rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.negative_year())
    {
        let task = &negative.tasks[&format!("t{index}")];
        assert_eq!(task.started, Some(row.instant), "{}", row.label);
    }
    store.delete_run("golden-negative").await.unwrap();

    // Reader: rows as the old code wrote them, plus the other two text forms.
    let mut listed = Vec::new();
    for row in rows.iter().filter(|row| !row.negative_year()) {
        let id = run_id(row);
        execute(
            &pool,
            format!(
                "INSERT INTO {} (id, flow_name, status, started, started_micros, finished, ctx) \
                 VALUES ('{id}', 'f', 'success', '{}', {}, '{}', '{{}}')",
                target.runs, row.sql, row.micros, row.sql
            ),
        )
        .await;
        listed.push((row.micros, id.clone()));
        for (kind, text) in [("serde", row.serde), ("event", row.event)] {
            let id = format!("{kind}-{}", row.label.replace('_', "-"));
            execute(
                &pool,
                format!(
                    "INSERT INTO {} (id, flow_name, status, started, started_micros, finished, ctx) \
                     VALUES ('{id}', 'f', 'failed', '{text}', {}, NULL, '{{}}')",
                    target.runs, row.micros
                ),
            )
            .await;
            assert_eq!(
                store.get_run_info(&id).await.unwrap().started,
                Some(row.instant),
                "{id}"
            );
        }
        let info = store.get_run_info(&id).await.unwrap();
        assert_eq!(info.started, Some(row.instant), "{id}");
        assert_eq!(info.finished, Some(row.instant), "{id}");
    }
    assert_eq!(
        all_pages(&store, Some(RunStatus::Success), 7).await,
        listing_order(listed.clone())
    );

    // Rows from before the microsecond column existed are backfilled on open.
    for row in rows.iter().filter(|row| !row.negative_year()) {
        execute(
            &pool,
            format!(
                "INSERT INTO {} (id, flow_name, status, started, started_micros, finished, ctx) \
                 VALUES ('legacy-{}', 'f', 'stalled', '{}', NULL, NULL, '{{}}')",
                target.runs,
                row.label.replace('_', "-"),
                row.sql
            ),
        )
        .await;
    }
    drop(store);
    let store = target.store().await;
    for row in rows.iter().filter(|row| !row.negative_year()) {
        let backfilled = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT started_micros FROM {} WHERE id = 'legacy-{}'",
            target.runs,
            row.label.replace('_', "-")
        )))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            backfilled.get::<i64, _>("started_micros"),
            row.micros,
            "{}",
            row.label
        );
    }

    // Retention compares floored microseconds, strictly: a cutoff inside a
    // run's own microsecond keeps the run.
    let cutoff = rows
        .iter()
        .find(|row| row.label == "before_epoch_sub_micro")
        .unwrap();
    let older = rows
        .iter()
        .filter(|row| !row.negative_year() && row.micros < cutoff.micros)
        .count();
    assert!(older >= 4);
    assert_eq!(store.prune_before(cutoff.instant).await.unwrap(), older * 4);
    assert!(store.get_run_info(&run_id(cutoff)).await.is_ok());
    assert!(store.get_run_info("row-year-1900").await.is_err());
    let kept: Vec<(i64, String)> = listed
        .into_iter()
        .filter(|(micros, _)| *micros >= cutoff.micros)
        .collect();
    assert_eq!(
        all_pages(&store, Some(RunStatus::Success), 100).await,
        listing_order(kept)
    );
    pool.close().await;
}

#[tokio::test]
async fn sqlite_state_store_keeps_its_timestamp_forms() {
    let dir = tempfile::tempdir().unwrap();
    assert_sql_state_store_forms(SqlTarget {
        url: format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("state.sqlite").to_string_lossy()
        ),
        runs: "ironflow_runs".to_string(),
        tasks: "ironflow_tasks".to_string(),
        prefix: None,
    })
    .await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_state_store_keeps_its_timestamp_forms() {
    let Some(fixture) = postgres_support::PostgresStateTest::from_env("ts_forms") else {
        return;
    };
    // Create and drop through the fixture so the tables are always removed.
    drop(fixture.state_store().await.unwrap());
    let prefix = fixture.table("");
    assert_sql_state_store_forms(SqlTarget {
        url: fixture.url().to_string(),
        runs: fixture.table("runs"),
        tasks: fixture.table("tasks"),
        prefix: Some(prefix),
    })
    .await;
    fixture.cleanup().await.unwrap();
}

// ===== SQL event store =====

fn event(run: &str, row: &Row) -> RunEvent {
    let mut event = RunEvent::run(run, "f", RunEventType::RunStarted, RunStatus::Running);
    event.id = row.label.to_string();
    event.timestamp = row.instant;
    event
}

async fn assert_sql_event_store_forms(url: &str, prefix: Option<&str>, events_table: &str) {
    sqlx::any::install_default_drivers();
    let rows = rows();
    let store = SqlEventStore::new_with_prefix(url, prefix).await.unwrap();
    let pool = sqlx::AnyPool::connect(url).await.unwrap();

    let mut expected = Vec::new();
    let mut negative = Vec::new();
    for row in &rows {
        let run = if row.negative_year() {
            "golden-negative"
        } else {
            "golden"
        };
        let event = event(run, row);
        store.publish(event.clone()).await.unwrap();
        if row.negative_year() {
            negative.push(event);
        } else {
            expected.push(event);
        }
    }
    let written = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT id, timestamp, event_json FROM {events_table}"
    )))
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(written.len(), rows.len());
    for stored in written {
        let id: String = stored.get("id");
        let row = rows.iter().find(|row| row.label == id).unwrap();
        assert_eq!(stored.get::<String, _>("timestamp"), row.event, "{id}");
        let json: String = stored.get("event_json");
        assert!(
            json.contains(&format!(r#""timestamp":"{}""#, row.serde)),
            "{json}"
        );
    }
    assert_eq!(
        store.list_since("golden", None, 100).await.unwrap(),
        expected
    );
    // Accepted difference: as in the state store, the old reader rejected
    // the signed year its own writer produced. It reads now.
    assert_eq!(negative.len(), 2);
    assert_eq!(
        store
            .list_since("golden-negative", None, 100)
            .await
            .unwrap(),
        negative
    );

    // A row whose column and payload spell the same instant differently, as
    // two replicas with different formatters would have written it.
    let row = rows.iter().find(|row| row.label == "half_second").unwrap();
    let mut mixed = event("golden-mixed", row);
    mixed.id = "mixed".to_string();
    store.publish(mixed.clone()).await.unwrap();
    execute(
        &pool,
        format!(
            "UPDATE {events_table} SET timestamp = '2026-01-02T03:04:05.5Z' WHERE id = 'mixed'"
        ),
    )
    .await;
    assert_eq!(
        store.list_since("golden-mixed", None, 10).await.unwrap(),
        vec![mixed]
    );
    execute(
        &pool,
        format!("UPDATE {events_table} SET timestamp = '2026-01-02T03:04:06Z' WHERE id = 'mixed'"),
    )
    .await;
    assert!(
        store.list_since("golden-mixed", None, 10).await.is_err(),
        "a column that names another instant is corruption"
    );
    pool.close().await;
}

#[tokio::test]
async fn sqlite_event_store_keeps_its_timestamp_forms() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("events.sqlite").to_string_lossy()
    );
    assert_sql_event_store_forms(&url, None, "ironflow_events").await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_event_store_keeps_its_timestamp_forms() {
    let Some(fixture) = postgres_support::PostgresStateTest::from_env("ts_events") else {
        return;
    };
    let prefix = fixture.table("");
    let events = fixture.table("events");
    assert_sql_event_store_forms(fixture.url(), Some(&prefix), &events).await;
    let pool = sqlx::AnyPool::connect(fixture.url()).await.unwrap();
    for suffix in ["events", "event_sequences", "event_deletions"] {
        execute(
            &pool,
            format!("DROP TABLE IF EXISTS {} CASCADE", fixture.table(suffix)),
        )
        .await;
    }
    pool.close().await;
}
