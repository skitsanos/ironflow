use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::StorageErrorKind;
use ironflow::storage::event_store::EventStore;

use super::redis_support::RedisTest;

#[tokio::test]
async fn redis_event_owner_marker_fences_a_historical_raw_encoding_collision() {
    let Some(fixture) = RedisTest::connect("event_historical_encoding_collision").await else {
        return;
    };
    // `:` encodes to `~3a`, which a pre-upgrade run could have used verbatim.
    let historical_id = "~3a";
    let historical_base = format!("{}events:{historical_id}", fixture.prefix);
    let historical = RunEvent::run(
        historical_id,
        "historical-flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::pipe()
        .cmd("RPUSH")
        .arg(&historical_base)
        .arg(serde_json::to_string(&historical).unwrap())
        .cmd("HSET")
        .arg(format!("{historical_base}:index"))
        .arg(&historical.id)
        .arg(1)
        .cmd("SET")
        .arg(format!("{historical_base}:seq"))
        .arg(1)
        .cmd("HSET")
        .arg(format!("{historical_base}:meta"))
        .arg("layout")
        .arg("2")
        .query_async(&mut conn)
        .await
        .unwrap();

    let store = fixture.event_store(None).await;
    let colliding = RunEvent::run(
        ":",
        "colliding-flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    assert!(store.publish(colliding.clone()).await.is_err());
    let unchanged: Vec<String> = redis::cmd("LRANGE")
        .arg(&historical_base)
        .arg(0)
        .arg(-1)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(unchanged, vec![serde_json::to_string(&historical).unwrap()]);

    // The ownerless physical key is intentionally ambiguous. An operator can
    // authorize its historical owner explicitly before bounded migration.
    let _: usize = redis::cmd("HSET")
        .arg(format!("{historical_base}:meta"))
        .arg("run_id")
        .arg(historical_id)
        .query_async(&mut conn)
        .await
        .unwrap();

    // Accessing the explicitly marked historical owner migrates it to its own encoded family;
    // the formerly colliding ID can then use the vacated key safely.
    assert_eq!(
        store.list_since(historical_id, None, 10).await.unwrap(),
        vec![historical.clone()]
    );
    let migrated_owner: String = redis::cmd("HGET")
        .arg(format!(
            "{}events:~{}:meta",
            fixture.prefix,
            hex::encode(historical_id)
        ))
        .arg("run_id")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(migrated_owner, historical_id);
    store.publish(colliding.clone()).await.unwrap();
    assert_eq!(
        store.list_since(":", None, 10).await.unwrap(),
        vec![colliding]
    );
    assert_eq!(
        store.list_since(historical_id, None, 10).await.unwrap(),
        vec![historical]
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_ownerless_empty_unsafe_family_is_never_claimed_or_deleted() {
    let Some(fixture) = RedisTest::connect("event_empty_encoding_collision").await else {
        return;
    };
    let run_id = ":";
    let ambiguous_base = format!("{}events:~3a", fixture.prefix);
    let sequence = format!("{ambiguous_base}:seq");
    let mut conn = fixture.connection().await.unwrap();
    redis::cmd("SET")
        .arg(&sequence)
        .arg(0)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();

    let store = fixture.event_store(None).await;
    for error in [
        store.list_since(run_id, None, 1).await.unwrap_err(),
        store.delete_run(run_id).await.unwrap_err(),
    ] {
        assert_eq!(error.kind(), StorageErrorKind::Corruption);
        assert!(error.diagnostic().contains("Ambiguous"));
    }
    let value: String = redis::cmd("GET")
        .arg(&sequence)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(value, "0");
    let marker_exists: bool = redis::cmd("EXISTS")
        .arg(format!("{ambiguous_base}:meta"))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!marker_exists);
    fixture.cleanup().await;
}
