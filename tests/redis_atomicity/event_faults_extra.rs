use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::event_store::EventStore;

use super::RedisTest;

const PUBLISH_SCRIPT: &str = include_str!("../../src/storage/event_store/scripts/publish.lua");

async fn seed_legacy(
    conn: &mut redis::aio::ConnectionManager,
    base: &str,
    event_id: &str,
    raw: &str,
) {
    let _: () = redis::pipe()
        .cmd("RPUSH")
        .arg(base)
        .arg(raw)
        .cmd("HSET")
        .arg(format!("{base}:index"))
        .arg(event_id)
        .arg(1)
        .cmd("SET")
        .arg(format!("{base}:seq"))
        .arg(1)
        .query_async(conn)
        .await
        .unwrap();
}

#[tokio::test]
async fn redis_legacy_marker_requires_full_payload_and_matching_run() {
    let Some(fixture) = RedisTest::connect("event_legacy_schema").await else {
        return;
    };
    let store = fixture.event_store(None).await;
    let mut conn = fixture.connection().await.unwrap();

    let malformed_run = "malformed-legacy";
    let malformed_base = format!("{}events:{malformed_run}", fixture.prefix);
    seed_legacy(
        &mut conn,
        &malformed_base,
        "malformed-id",
        r#"{"id":"malformed-id","run_id":"malformed-legacy"}"#,
    )
    .await;
    assert!(store.list_since(malformed_run, None, 10).await.is_err());
    let malformed_marker: bool = redis::cmd("EXISTS")
        .arg(format!("{malformed_base}:meta"))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!malformed_marker, "invalid payload gained a layout marker");

    let expected_run = "expected-legacy";
    let cross_run = RunEvent::run(
        "different-run",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let cross_base = format!("{}events:{expected_run}", fixture.prefix);
    seed_legacy(
        &mut conn,
        &cross_base,
        &cross_run.id,
        &serde_json::to_string(&cross_run).unwrap(),
    )
    .await;
    assert!(store.list_since(expected_run, None, 10).await.is_err());
    let cross_marker: bool = redis::cmd("EXISTS")
        .arg(format!("{cross_base}:meta"))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!cross_marker, "cross-run payload gained a layout marker");

    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_publish_rejects_empty_ids_aliased_keys_and_noncanonical_sequence() {
    let Some(fixture) = RedisTest::connect("event_publish_guards").await else {
        return;
    };
    let store = fixture.event_store(None).await;
    let mut conn = fixture.connection().await.unwrap();

    let mut empty_id = RunEvent::run(
        "empty-id-run",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    empty_id.id.clear();
    assert!(store.publish(empty_id).await.is_err());
    let empty_base = format!("{}events:empty-id-run", fixture.prefix);
    let empty_keys: usize = redis::cmd("EXISTS")
        .arg(&empty_base)
        .arg(format!("{empty_base}:index"))
        .arg(format!("{empty_base}:seq"))
        .arg(format!("{empty_base}:meta"))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(empty_keys, 0);

    let alias_event = RunEvent::run(
        "alias-guard",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let alias_json = serde_json::to_string(&alias_event).unwrap();
    let alias_key = format!("{}aliased-event-key", fixture.prefix);
    let alias_seq = format!("{alias_key}:seq");
    let alias_meta = format!("{alias_key}:meta");
    let alias_fence = format!("{}event_deletions:v1:alias-guard", fixture.prefix);
    let alias_result: redis::RedisResult<i64> = redis::Script::new(PUBLISH_SCRIPT)
        .key(&alias_key)
        .key(&alias_key)
        .key(&alias_seq)
        .key(&alias_meta)
        .key(&alias_fence)
        .arg(&alias_json)
        .arg(&alias_event.id)
        .arg(&alias_event.run_id)
        .arg(-1)
        .invoke_async(&mut conn)
        .await;
    assert!(alias_result.is_err());
    let alias_keys: usize = redis::cmd("EXISTS")
        .arg(&alias_key)
        .arg(&alias_seq)
        .arg(&alias_meta)
        .arg(&alias_fence)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(alias_keys, 0, "aliased script keys left partial state");

    let sequence_run = "sequence-guard";
    let original = RunEvent::run(
        sequence_run,
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let sequence_base = format!("{}events:{sequence_run}", fixture.prefix);
    seed_legacy(
        &mut conn,
        &sequence_base,
        &original.id,
        &serde_json::to_string(&original).unwrap(),
    )
    .await;
    let _: () = redis::cmd("SET")
        .arg(format!("{sequence_base}:seq"))
        .arg("01")
        .query_async(&mut conn)
        .await
        .unwrap();
    let next = RunEvent::run(
        sequence_run,
        "flow",
        RunEventType::RunFinished,
        RunStatus::Success,
    );
    let sequence_result: redis::RedisResult<i64> = redis::Script::new(PUBLISH_SCRIPT)
        .key(&sequence_base)
        .key(format!("{sequence_base}:index"))
        .key(format!("{sequence_base}:seq"))
        .key(format!("{sequence_base}:meta"))
        .key(format!(
            "{}event_deletions:v1:{sequence_run}",
            fixture.prefix
        ))
        .arg(serde_json::to_string(&next).unwrap())
        .arg(&next.id)
        .arg(sequence_run)
        .arg(-1)
        .invoke_async(&mut conn)
        .await;
    assert!(sequence_result.is_err());
    let (raw_seq, list_len, index_len, has_meta): (String, usize, usize, bool) = redis::pipe()
        .cmd("GET")
        .arg(format!("{sequence_base}:seq"))
        .cmd("LLEN")
        .arg(&sequence_base)
        .cmd("HLEN")
        .arg(format!("{sequence_base}:index"))
        .cmd("EXISTS")
        .arg(format!("{sequence_base}:meta"))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        (raw_seq.as_str(), list_len, index_len, has_meta),
        ("01", 1, 1, false)
    );

    fixture.cleanup().await;
}
