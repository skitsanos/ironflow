use std::collections::HashMap;

use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::StorageErrorKind;
use ironflow::storage::event_store::EventStore;

use super::redis_support::RedisTest;

const LEGACY_BATCH_BYTES: usize = 1_048_576;
const LEGACY_COMMON: &str = include_str!("../../src/storage/event_store/scripts/legacy_common.lua");
const LEGACY_STATUS: &str = include_str!("../../src/storage/event_store/scripts/legacy_status.lua");
const LEGACY_FETCH: &str = include_str!("../../src/storage/event_store/scripts/legacy_fetch.lua");
const LEGACY_COMMIT: &str = include_str!("../../src/storage/event_store/scripts/legacy_commit.lua");
const LEGACY_TRANSITION: &str =
    include_str!("../../src/storage/event_store/scripts/legacy_transition.lua");

fn event(run_id: &str, id: impl Into<String>) -> RunEvent {
    let mut event = RunEvent::run(
        run_id,
        "legacy-recovery",
        RunEventType::ContextUpdated,
        RunStatus::Running,
    );
    event.id = id.into();
    event
}

fn migration_script(body: &str) -> redis::Script {
    redis::Script::new(&format!("{LEGACY_COMMON}\n{body}"))
}

fn prepare_migration<'a>(
    script: &'a redis::Script,
    keys: &[String; 15],
) -> redis::ScriptInvocation<'a> {
    let mut invocation = script.prepare_invoke();
    for key in keys {
        invocation.key(key);
    }
    invocation
}

async fn dump_family(conn: &mut redis::aio::ConnectionManager, base: &str) -> Vec<Option<Vec<u8>>> {
    let mut result = Vec::with_capacity(4);
    for key in [
        base.to_string(),
        format!("{base}:index"),
        format!("{base}:seq"),
        format!("{base}:meta"),
    ] {
        result.push(redis::cmd("DUMP").arg(key).query_async(conn).await.unwrap());
    }
    result
}

async fn migration_state(
    conn: &mut redis::aio::ConnectionManager,
    state_key: &str,
) -> HashMap<String, String> {
    redis::cmd("HGETALL")
        .arg(state_key)
        .query_async(conn)
        .await
        .unwrap()
}

async fn transition_state(
    conn: &mut redis::aio::ConnectionManager,
    script: &redis::Script,
    keys: &[String; 15],
    owner: &str,
) -> Vec<String> {
    let state = migration_state(conn, &keys[8]).await;
    prepare_migration(script, keys)
        .arg(owner)
        .arg(&state["token"])
        .arg(&state["generation"])
        .arg(&state["phase"])
        .arg(&state["cursor"])
        .arg(&state["digest"])
        .invoke_async(conn)
        .await
        .unwrap()
}

async fn fetch_batch(
    conn: &mut redis::aio::ConnectionManager,
    script: &redis::Script,
    keys: &[String; 15],
    owner: &str,
    progress: &[String],
) -> Vec<Vec<u8>> {
    prepare_migration(script, keys)
        .arg(owner)
        .arg(&progress[2])
        .arg(&progress[4])
        .arg(&progress[1])
        .arg(&progress[5])
        .arg(&progress[9])
        .invoke_async(conn)
        .await
        .unwrap()
}

async fn commit_batch(
    conn: &mut redis::aio::ConnectionManager,
    script: &redis::Script,
    keys: &[String; 15],
    owner: &str,
    progress: &[String],
    batch: &[Vec<u8>],
) {
    let next_cursor = std::str::from_utf8(&batch[2]).unwrap();
    let batch_digest = std::str::from_utf8(&batch[3]).unwrap();
    let response: Vec<String> = prepare_migration(script, keys)
        .arg(owner)
        .arg(&progress[2])
        .arg(&progress[4])
        .arg(&progress[1])
        .arg(&progress[5])
        .arg(&progress[9])
        .arg(next_cursor)
        .arg(batch_digest)
        .invoke_async(conn)
        .await
        .unwrap();
    assert_eq!(response.first().map(String::as_str), Some("pending"));
}

#[tokio::test]
async fn redis_legacy_migration_restores_an_invalid_utf8_event_byte_for_byte() {
    let Some(fixture) = RedisTest::connect("event_legacy_invalid_utf8").await else {
        return;
    };
    let run_id = "legacy-invalid-utf8";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let stored = event(run_id, "legacy-invalid-utf8-id");
    let mut raw = serde_json::to_vec(&stored).unwrap();
    let flow_name = b"legacy-recovery";
    let invalid_offset = raw
        .windows(flow_name.len())
        .position(|window| window == flow_name)
        .expect("serialized event is missing its flow name");
    raw[invalid_offset] = 0xff;

    let mut conn = fixture.connection().await.unwrap();
    redis::pipe()
        .cmd("RPUSH")
        .arg(&base)
        .arg(&raw)
        .cmd("HSET")
        .arg(format!("{base}:index"))
        .arg(&stored.id)
        .arg(1)
        .cmd("SET")
        .arg(format!("{base}:seq"))
        .arg(1)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();
    let before = dump_family(&mut conn, &base).await;

    let error = fixture
        .event_store(None)
        .await
        .list_since(run_id, None, 1)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Corruption);
    assert_eq!(dump_family(&mut conn, &base).await, before);
    let marker: Option<String> = redis::cmd("HGET")
        .arg(format!("{base}:meta"))
        .arg("layout")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(marker.is_none(), "invalid event received an owner marker");
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_legacy_reverse_restore_respects_the_aggregate_byte_limit() {
    const VALID_EVENTS: usize = 34;
    const PAYLOAD_BYTES: usize = 530_000;
    let Some(fixture) = RedisTest::connect("event_legacy_restore_bytes").await else {
        return;
    };
    let run_id = "legacy-restore-bytes";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let index = format!("{base}:index");
    let mut conn = fixture.connection().await.unwrap();
    let mut pipe = redis::pipe();
    for position in 0..VALID_EVENTS {
        let mut stored = event(run_id, format!("legacy-restore-byte-{position:02}"));
        stored.reason = Some("x".repeat(PAYLOAD_BYTES));
        let raw = serde_json::to_vec(&stored).unwrap();
        assert!(raw.len() < LEGACY_BATCH_BYTES);
        pipe.cmd("RPUSH")
            .arg(&base)
            .arg(raw)
            .cmd("HSET")
            .arg(&index)
            .arg(&stored.id)
            .arg(position + 1);
    }
    pipe.cmd("RPUSH")
        .arg(&base)
        .arg(b"{not-json".as_slice())
        .cmd("HSET")
        .arg(&index)
        .arg("legacy-restore-byte-invalid")
        .arg(VALID_EVENTS + 1)
        .cmd("SET")
        .arg(format!("{base}:seq"))
        .arg(VALID_EVENTS + 1)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();
    let before = dump_family(&mut conn, &base).await;
    let store = fixture.event_store(None).await;

    assert_eq!(
        store.list_since(run_id, None, 1).await.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );
    assert_eq!(
        store.list_since(run_id, None, 1).await.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );
    let state = format!(
        "{}event_migrations:v1:{}",
        fixture.prefix,
        hex::encode(run_id)
    );
    let (phase, cursor, pending_count): (String, usize, Option<usize>) = redis::pipe()
        .cmd("HGET")
        .arg(&state)
        .arg("phase")
        .cmd("HGET")
        .arg(&state)
        .arg("cursor")
        .cmd("HGET")
        .arg(&state)
        .arg("pending_count")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!((phase.as_str(), pending_count), ("restore", None));
    assert!(
        (1..=4).contains(&cursor),
        "byte-bounded restoration made unexpected progress: {cursor} events remain"
    );

    let restored = store.list_since(run_id, None, 1).await.unwrap_err();
    assert_eq!(restored.kind(), StorageErrorKind::Corruption);
    assert_eq!(dump_family(&mut conn, &base).await, before);
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_final_verify_ack_closes_the_snapshot_before_returning() {
    let Some(fixture) = RedisTest::connect("event_legacy_verify_closure").await else {
        return;
    };
    let run_id = "legacy-verify-closure";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let migration = format!(
        "{}event_migrations:v1:{}",
        fixture.prefix,
        hex::encode(run_id)
    );
    let snapshot = format!("{migration}:snapshot");
    let current = [
        base.clone(),
        format!("{base}:index"),
        format!("{base}:seq"),
        format!("{base}:meta"),
    ];
    let frozen = [
        snapshot.clone(),
        format!("{snapshot}:index"),
        format!("{snapshot}:seq"),
        format!("{snapshot}:meta"),
    ];
    let probe = uuid::Uuid::new_v4().simple().to_string();
    let keys = [
        current[0].clone(),
        current[1].clone(),
        current[2].clone(),
        current[3].clone(),
        current[0].clone(),
        current[1].clone(),
        current[2].clone(),
        current[3].clone(),
        migration.clone(),
        frozen[0].clone(),
        frozen[1].clone(),
        frozen[2].clone(),
        frozen[3].clone(),
        format!("{}event_migration_probes:v1:{probe}:from", fixture.prefix),
        format!("{}event_migration_probes:v1:{probe}:to", fixture.prefix),
    ];
    let stored = event(run_id, "legacy-verify-closure-id");
    let raw = serde_json::to_string(&stored).unwrap();
    let mut conn = fixture.connection().await.unwrap();
    redis::pipe()
        .cmd("RPUSH")
        .arg(&base)
        .arg(&raw)
        .cmd("HSET")
        .arg(&current[1])
        .arg(&stored.id)
        .arg(1)
        .cmd("SET")
        .arg(&current[2])
        .arg(1)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();

    let status_script = migration_script(LEGACY_STATUS);
    let fetch_script = migration_script(LEGACY_FETCH);
    let commit_script = migration_script(LEGACY_COMMIT);
    let transition_script = migration_script(LEGACY_TRANSITION);
    let scan: Vec<String> = prepare_migration(&status_script, &keys)
        .arg(run_id)
        .arg("0123456789abcdef0123456789abcdef")
        .arg(0)
        .arg(128)
        .arg(LEGACY_BATCH_BYTES)
        .invoke_async(&mut conn)
        .await
        .unwrap();
    let scan_batch = fetch_batch(&mut conn, &fetch_script, &keys, run_id, &scan).await;
    commit_batch(&mut conn, &commit_script, &keys, run_id, &scan, &scan_batch).await;
    let scan_done = transition_state(&mut conn, &transition_script, &keys, run_id).await;
    assert_eq!(
        (scan_done[1].as_str(), scan_done[5].as_str()),
        ("scan", "1")
    );
    let verify = transition_state(&mut conn, &transition_script, &keys, run_id).await;
    assert_eq!((verify[1].as_str(), verify[5].as_str()), ("verify", "0"));
    let verify_batch = fetch_batch(&mut conn, &fetch_script, &keys, run_id, &verify).await;
    commit_batch(
        &mut conn,
        &commit_script,
        &keys,
        run_id,
        &verify,
        &verify_batch,
    )
    .await;
    assert_eq!(
        migration_state(&mut conn, &migration).await["phase"],
        "verify_pending"
    );

    let closed = transition_state(&mut conn, &transition_script, &keys, run_id).await;
    assert_eq!(closed, vec!["current"]);
    assert!(migration_state(&mut conn, &migration).await.is_empty());
    let tamper: redis::RedisResult<()> = redis::cmd("LSET")
        .arg(&snapshot)
        .arg(0)
        .arg("tampered-after-final-ack")
        .query_async(&mut conn)
        .await;
    assert!(
        tamper.is_err(),
        "the final acknowledgement exposed a snapshot window"
    );
    assert_eq!(
        fixture
            .event_store(None)
            .await
            .list_since(run_id, None, 1)
            .await
            .unwrap(),
        vec![stored]
    );
    fixture.cleanup().await;
}
