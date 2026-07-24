use std::collections::HashMap;
use std::time::Duration;

use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::event_store::{EventStore, RedisEventStore};
use ironflow::storage::{StorageErrorKind, StorageResult};

use super::redis_support::RedisTest;

const BATCH_SIZE: usize = 128;
const STEPS_PER_OPERATION: usize = 32;
const EVENTS_PER_OPERATION: usize = BATCH_SIZE * STEPS_PER_OPERATION;
const OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const PAGE_SIZE: usize = 257;
const SEED_CHUNK: usize = 500;
type LogicalFamily = (
    Vec<String>,
    HashMap<String, String>,
    Option<String>,
    HashMap<String, String>,
);

#[derive(Clone)]
struct EventKeys {
    list: String,
    index: String,
    sequence: String,
    meta: String,
    migration: String,
}

impl EventKeys {
    fn new(prefix: &str, run_id: &str) -> Self {
        let list = format!("{prefix}events:{run_id}");
        Self {
            index: format!("{list}:index"),
            sequence: format!("{list}:seq"),
            meta: format!("{list}:meta"),
            migration: format!(
                "{prefix}event_migrations:v1:{}",
                hex::encode(run_id.as_bytes())
            ),
            list,
        }
    }

    fn family(&self) -> [&str; 4] {
        [&self.list, &self.index, &self.sequence, &self.meta]
    }

    fn snapshot(&self) -> Self {
        let list = format!("{}:snapshot", self.migration);
        Self {
            index: format!("{list}:index"),
            sequence: format!("{list}:seq"),
            meta: format!("{list}:meta"),
            migration: self.migration.clone(),
            list,
        }
    }
}

fn legacy_event(run_id: &str, index: usize) -> RunEvent {
    let mut event = RunEvent::run(
        run_id,
        "legacy-bounded-flow",
        RunEventType::ContextUpdated,
        RunStatus::Running,
    )
    .with_reason("original");
    event.id = format!("legacy-event-{index:05}");
    event
}

async fn seed_legacy(
    conn: &mut redis::aio::ConnectionManager,
    keys: &EventKeys,
    run_id: &str,
    count: usize,
) -> Vec<String> {
    let mut ids = Vec::with_capacity(count);
    for start in (0..count).step_by(SEED_CHUNK) {
        let mut pipe = redis::pipe();
        for index in start..(start + SEED_CHUNK).min(count) {
            let event = legacy_event(run_id, index);
            ids.push(event.id.clone());
            pipe.cmd("RPUSH")
                .arg(&keys.list)
                .arg(serde_json::to_string(&event).unwrap())
                .ignore();
            pipe.cmd("HSET")
                .arg(&keys.index)
                .arg(&event.id)
                .arg(index + 1)
                .ignore();
        }
        pipe.query_async::<()>(conn).await.unwrap();
    }
    redis::cmd("SET")
        .arg(&keys.sequence)
        .arg(count)
        .query_async::<()>(conn)
        .await
        .unwrap();
    ids
}

async fn list_with_timeout(
    store: &RedisEventStore,
    run_id: &str,
    after: Option<&str>,
    limit: usize,
) -> StorageResult<Vec<RunEvent>> {
    tokio::time::timeout(OPERATION_TIMEOUT, store.list_since(run_id, after, limit))
        .await
        .expect("bounded Redis legacy migration operation timed out")
}

async fn progress(
    conn: &mut redis::aio::ConnectionManager,
    keys: &EventKeys,
) -> HashMap<String, String> {
    redis::cmd("HGETALL")
        .arg(&keys.migration)
        .query_async(conn)
        .await
        .unwrap()
}

async fn assert_progress(
    conn: &mut redis::aio::ConnectionManager,
    keys: &EventKeys,
    phase: &str,
    cursor: usize,
    sequence: usize,
) -> String {
    let state = progress(conn, keys).await;
    assert_eq!(state.get("phase").map(String::as_str), Some(phase));
    assert_eq!(state["cursor"].parse::<usize>().unwrap(), cursor);
    assert_eq!(state["sequence"].parse::<usize>().unwrap(), sequence);
    assert_eq!(state["batch"].parse::<usize>().unwrap(), BATCH_SIZE);
    let token = state.get("token").expect("migration token is missing");
    assert_eq!(token.len(), 32);
    token.clone()
}

async fn logical_family(
    conn: &mut redis::aio::ConnectionManager,
    keys: &EventKeys,
) -> LogicalFamily {
    redis::pipe()
        .cmd("LRANGE")
        .arg(&keys.list)
        .arg(0)
        .arg(-1)
        .cmd("HGETALL")
        .arg(&keys.index)
        .cmd("GET")
        .arg(&keys.sequence)
        .cmd("HGETALL")
        .arg(&keys.meta)
        .query_async(conn)
        .await
        .unwrap()
}

async fn assert_no_migration_artifacts(conn: &mut redis::aio::ConnectionManager, keys: &EventKeys) {
    let artifacts: Vec<String> = redis::cmd("KEYS")
        .arg(format!("{}*", keys.migration))
        .query_async(conn)
        .await
        .unwrap();
    assert!(
        artifacts.is_empty(),
        "migration artifacts remain: {artifacts:?}"
    );
}

fn assert_conflict(result: StorageResult<Vec<RunEvent>>) {
    assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Conflict);
}

async fn collect_ids(
    store: &RedisEventStore,
    run_id: &str,
    first_page: Vec<RunEvent>,
) -> Vec<String> {
    let mut page = first_page;
    let mut ids = Vec::new();
    loop {
        if page.is_empty() {
            return ids;
        }
        let after = page.last().unwrap().id.clone();
        ids.extend(page.into_iter().map(|event| event.id));
        page = list_with_timeout(store, run_id, Some(&after), PAGE_SIZE)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn redis_large_legacy_event_migration_is_bounded_resumable_and_ordered() {
    const EVENT_COUNT: usize = 10_000;
    let Some(fixture) = RedisTest::connect("event_legacy_large_bounded").await else {
        return;
    };
    let run_id = "legacy-large-bounded";
    let keys = EventKeys::new(&fixture.prefix, run_id);
    let mut conn = fixture.connection().await.unwrap();
    let expected = seed_legacy(&mut conn, &keys, run_id, EVENT_COUNT).await;
    let first_store = fixture.event_store(None).await;
    assert_conflict(list_with_timeout(&first_store, run_id, None, PAGE_SIZE).await);
    let token = assert_progress(&mut conn, &keys, "scan", EVENTS_PER_OPERATION, EVENT_COUNT).await;
    drop(first_store);
    let resumed = fixture.event_store(None).await;
    for (phase, cursor) in [("scan", 8192), ("verify", 2176), ("verify", 6272)] {
        assert_conflict(list_with_timeout(&resumed, run_id, None, PAGE_SIZE).await);
        assert_eq!(
            assert_progress(&mut conn, &keys, phase, cursor, EVENT_COUNT).await,
            token,
            "a new migration token replaced resumable progress"
        );
    }
    let first_page = list_with_timeout(&resumed, run_id, None, PAGE_SIZE)
        .await
        .unwrap();
    assert_eq!(collect_ids(&resumed, run_id, first_page).await, expected);
    let marker: (String, String) = redis::pipe()
        .cmd("HGET")
        .arg(&keys.meta)
        .arg("layout")
        .cmd("HGET")
        .arg(&keys.meta)
        .arg("run_id")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(marker, ("2".to_string(), run_id.to_string()));
    assert_no_migration_artifacts(&mut conn, &keys).await;
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_late_legacy_corruption_never_marks_or_moves_the_source() {
    let Some(fixture) = RedisTest::connect("event_legacy_late_corruption").await else {
        return;
    };
    let mut conn = fixture.connection().await.unwrap();
    for (suffix, replacement) in [
        ("malformed", "{not-json".to_string()),
        ("cross-run", {
            let mut event = legacy_event("foreign-run", BATCH_SIZE + 17);
            event.id = format!("legacy-event-{:05}", BATCH_SIZE + 17);
            serde_json::to_string(&event).unwrap()
        }),
    ] {
        let run_id = format!("legacy-late-{suffix}");
        let keys = EventKeys::new(&fixture.prefix, &run_id);
        seed_legacy(&mut conn, &keys, &run_id, BATCH_SIZE * 3).await;
        redis::cmd("LSET")
            .arg(&keys.list)
            .arg(BATCH_SIZE + 17)
            .arg(replacement)
            .query_async::<()>(&mut conn)
            .await
            .unwrap();
        let before = logical_family(&mut conn, &keys).await;
        let store = fixture.event_store(None).await;
        let error = list_with_timeout(&store, &run_id, None, 1)
            .await
            .unwrap_err();
        assert_eq!(error.kind(), StorageErrorKind::Corruption);
        assert_eq!(logical_family(&mut conn, &keys).await, before);
        assert_no_migration_artifacts(&mut conn, &keys).await;
        let marker: Option<String> = redis::cmd("HGET")
            .arg(&keys.meta)
            .arg("layout")
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(marker.is_none(), "corrupt source received an owner marker");
    }
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_same_length_payload_tamper_forces_a_snapshot_rescan() {
    const EVENT_COUNT: usize = 5_000;
    let Some(fixture) = RedisTest::connect("event_legacy_payload_tamper").await else {
        return;
    };
    let run_id = "legacy-payload-tamper";
    let keys = EventKeys::new(&fixture.prefix, run_id);
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy(&mut conn, &keys, run_id, EVENT_COUNT).await;
    let store = fixture.event_store(None).await;
    assert_conflict(list_with_timeout(&store, run_id, None, 1).await);
    assert_progress(&mut conn, &keys, "scan", EVENTS_PER_OPERATION, EVENT_COUNT).await;
    let snapshot = keys.snapshot();
    let raw: String = redis::cmd("LINDEX")
        .arg(&snapshot.list)
        .arg(EVENT_COUNT - EVENTS_PER_OPERATION)
        .query_async(&mut conn)
        .await
        .unwrap();
    let mut changed: RunEvent = serde_json::from_str(&raw).unwrap();
    changed.reason = Some("tampered".to_string());
    let changed = serde_json::to_string(&changed).unwrap();
    assert_eq!(
        changed.len(),
        raw.len(),
        "tamper must preserve payload length"
    );
    redis::cmd("LSET")
        .arg(&snapshot.list)
        .arg(EVENT_COUNT - EVENTS_PER_OPERATION)
        .arg(&changed)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();
    assert_conflict(list_with_timeout(&store, run_id, None, 1).await);
    assert_progress(&mut conn, &keys, "verify", 3072, EVENT_COUNT).await;
    assert_conflict(list_with_timeout(&store, run_id, None, 1).await);
    assert_progress(&mut conn, &keys, "scan", 2048, EVENT_COUNT).await;
    let first = loop {
        match list_with_timeout(&store, run_id, None, 1).await {
            Ok(page) => break page,
            Err(error) if error.kind() == StorageErrorKind::Conflict => {}
            Err(error) => panic!("resumed payload migration failed: {error}"),
        }
    };
    assert_eq!(first[0].reason.as_deref(), Some("tampered"));
    assert_no_migration_artifacts(&mut conn, &keys).await;
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_index_tamper_is_rejected_in_snapshot_verification_and_can_resume() {
    const EVENT_COUNT: usize = 5_000;
    let Some(fixture) = RedisTest::connect("event_legacy_index_tamper").await else {
        return;
    };
    let run_id = "legacy-index-tamper";
    let keys = EventKeys::new(&fixture.prefix, run_id);
    let mut conn = fixture.connection().await.unwrap();
    let ids = seed_legacy(&mut conn, &keys, run_id, EVENT_COUNT).await;
    let store = fixture.event_store(None).await;
    assert_conflict(list_with_timeout(&store, run_id, None, 1).await);
    let snapshot = keys.snapshot();
    redis::cmd("HSET")
        .arg(&snapshot.index)
        .arg(&ids[0])
        .arg(2)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();
    let error = list_with_timeout(&store, run_id, None, 1)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Corruption);
    assert_no_migration_artifacts(&mut conn, &keys).await;
    let restored = logical_family(&mut conn, &keys).await;
    assert_eq!(restored.0.len(), EVENT_COUNT);
    assert_eq!(restored.1[&ids[0]], "2");
    assert!(restored.3.is_empty(), "tampered stream was owner-marked");
    redis::cmd("HSET")
        .arg(&keys.index)
        .arg(&ids[0])
        .arg(1)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();
    let first = loop {
        match list_with_timeout(&store, run_id, None, 1).await {
            Ok(page) => break page,
            Err(error) if error.kind() == StorageErrorKind::Conflict => {}
            Err(error) => panic!("repaired index migration failed: {error}"),
        }
    };
    assert_eq!(first[0].id, ids[0]);
    assert_no_migration_artifacts(&mut conn, &keys).await;
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_missing_migration_state_never_hides_a_deterministic_snapshot() {
    const EVENT_COUNT: usize = 5_000;
    let Some(fixture) = RedisTest::connect("event_legacy_orphan_snapshot").await else {
        return;
    };
    let run_id = "legacy-orphan-snapshot";
    let keys = EventKeys::new(&fixture.prefix, run_id);
    let snapshot = keys.snapshot();
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy(&mut conn, &keys, run_id, EVENT_COUNT).await;
    let store = fixture.event_store(None).await;
    assert_conflict(list_with_timeout(&store, run_id, None, 1).await);
    let quarantined = logical_family(&mut conn, &snapshot).await;
    assert_eq!(quarantined.0.len(), EVENT_COUNT);
    let deleted: usize = redis::cmd("DEL")
        .arg(&keys.migration)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(deleted, 1);
    let error = list_with_timeout(&store, run_id, None, 1)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Corruption);
    assert_eq!(logical_family(&mut conn, &snapshot).await, quarantined);
    let source_exists: usize = redis::cmd("EXISTS")
        .arg(&keys.family())
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(source_exists, 0, "orphaned snapshot was reported as empty");
    fixture.cleanup().await;
}
