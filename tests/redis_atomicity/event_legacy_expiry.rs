use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::StorageErrorKind;
use ironflow::storage::event_store::EventStore;

use super::redis_support::RedisTest;

fn event(run_id: &str, id: impl Into<String>) -> RunEvent {
    let mut event = RunEvent::run(
        run_id,
        "legacy-expiry",
        RunEventType::ContextUpdated,
        RunStatus::Running,
    );
    event.id = id.into();
    event
}

async fn seed_legacy(
    conn: &mut redis::aio::ConnectionManager,
    base: &str,
    run_id: &str,
    count: usize,
) {
    let index = format!("{base}:index");
    for start in (0..count).step_by(400) {
        let mut pipe = redis::pipe();
        for position in start..(start + 400).min(count) {
            let stored = event(run_id, format!("legacy-expiry-{position:05}"));
            pipe.cmd("RPUSH")
                .arg(base)
                .arg(serde_json::to_string(&stored).unwrap())
                .cmd("HSET")
                .arg(&index)
                .arg(&stored.id)
                .arg(position + 1);
        }
        pipe.query_async::<()>(conn).await.unwrap();
    }
    redis::cmd("SET")
        .arg(format!("{base}:seq"))
        .arg(count)
        .query_async::<()>(conn)
        .await
        .unwrap();
}

fn migration_key(prefix: &str, run_id: &str) -> String {
    format!("{prefix}event_migrations:v1:{}", hex::encode(run_id))
}

#[tokio::test]
async fn redis_legacy_restore_caps_hostile_ttl_extension_at_the_captured_deadline() {
    const EVENT_COUNT: usize = 4_200;
    let Some(fixture) = RedisTest::connect("event_legacy_restore_ttl").await else {
        return;
    };
    let run_id = "legacy-restore-ttl";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let family = [base.clone(), format!("{base}:index"), format!("{base}:seq")];
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy(&mut conn, &base, run_id, EVENT_COUNT).await;
    for key in &family {
        assert!(
            redis::cmd("PEXPIRE")
                .arg(key)
                .arg(60_000)
                .query_async::<bool>(&mut conn)
                .await
                .unwrap()
        );
    }
    let initial_ttl: i64 = redis::cmd("PTTL")
        .arg(&base)
        .query_async(&mut conn)
        .await
        .unwrap();
    let store = fixture.event_store(None).await;
    assert_eq!(
        store.list_since(run_id, None, 1).await.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );

    let migration = migration_key(&fixture.prefix, run_id);
    let snapshot = format!("{migration}:snapshot");
    for key in [
        snapshot.clone(),
        format!("{snapshot}:index"),
        format!("{snapshot}:seq"),
    ] {
        assert!(
            redis::cmd("PERSIST")
                .arg(key)
                .query_async::<bool>(&mut conn)
                .await
                .unwrap()
        );
    }
    let head: String = redis::cmd("LINDEX")
        .arg(&snapshot)
        .arg(0)
        .query_async(&mut conn)
        .await
        .unwrap();
    let head: RunEvent = serde_json::from_str(&head).unwrap();
    redis::cmd("HSET")
        .arg(format!("{snapshot}:index"))
        .arg(&head.id)
        .arg(1)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();

    let error = store.list_since(run_id, None, 1).await.unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Corruption);
    let mut restored_ttls = Vec::new();
    for key in &family {
        restored_ttls.push(
            redis::cmd("PTTL")
                .arg(key)
                .query_async::<i64>(&mut conn)
                .await
                .unwrap(),
        );
    }
    assert!(
        restored_ttls
            .iter()
            .all(|ttl| *ttl > 0 && *ttl <= initial_ttl),
        "restoration revived a captured TTL: {restored_ttls:?}"
    );
    assert!(
        !redis::cmd("EXISTS")
            .arg(&migration)
            .query_async::<bool>(&mut conn)
            .await
            .unwrap()
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_hostile_persist_past_the_deadline_expires_instead_of_restoring() {
    const EVENT_COUNT: usize = 4_200;
    let Some(fixture) = RedisTest::connect("event_legacy_restore_expired_ttl").await else {
        return;
    };
    let run_id = "legacy-restore-expired-ttl";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy(&mut conn, &base, run_id, EVENT_COUNT).await;
    for key in [&base, &format!("{base}:index"), &format!("{base}:seq")] {
        assert!(
            redis::cmd("PEXPIRE")
                .arg(key)
                .arg(1_000)
                .query_async::<bool>(&mut conn)
                .await
                .unwrap()
        );
    }
    let store = fixture.event_store(None).await;
    assert_eq!(
        store.list_since(run_id, None, 1).await.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );
    let migration = migration_key(&fixture.prefix, run_id);
    let snapshot = format!("{migration}:snapshot");
    for key in [
        snapshot.clone(),
        format!("{snapshot}:index"),
        format!("{snapshot}:seq"),
    ] {
        assert!(
            redis::cmd("PERSIST")
                .arg(key)
                .query_async::<bool>(&mut conn)
                .await
                .unwrap()
        );
    }
    let head: String = redis::cmd("LINDEX")
        .arg(&snapshot)
        .arg(0)
        .query_async(&mut conn)
        .await
        .unwrap();
    let head: RunEvent = serde_json::from_str(&head).unwrap();
    redis::cmd("HSET")
        .arg(format!("{snapshot}:index"))
        .arg(head.id)
        .arg(1)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
    let expiring = store.list_since(run_id, None, 1).await.unwrap_err();
    assert_eq!(expiring.kind(), StorageErrorKind::Conflict);
    assert!(expiring.diagnostic().contains("expiring"));
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert!(store.list_since(run_id, None, 1).await.unwrap().is_empty());
    assert_eq!(
        redis::cmd("EXISTS")
            .arg(&base)
            .arg(format!("{base}:index"))
            .arg(format!("{base}:seq"))
            .arg(&migration)
            .arg(&snapshot)
            .arg(format!("{snapshot}:index"))
            .arg(format!("{snapshot}:seq"))
            .query_async::<usize>(&mut conn)
            .await
            .unwrap(),
        0
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_expired_legacy_quarantine_releases_its_persistent_state() {
    const EVENT_COUNT: usize = 10_000;
    let Some(fixture) = RedisTest::connect("event_legacy_expired_quarantine").await else {
        return;
    };
    let run_id = "legacy-expired-quarantine";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy(&mut conn, &base, run_id, EVENT_COUNT).await;
    assert!(
        redis::cmd("PEXPIRE")
            .arg(&base)
            .arg(2_000)
            .query_async::<bool>(&mut conn)
            .await
            .unwrap()
    );
    let store = fixture.event_store(None).await;
    assert_eq!(
        store.list_since(run_id, None, 1).await.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );
    let migration = migration_key(&fixture.prefix, run_id);
    let snapshot = format!("{migration}:snapshot");
    for key in [
        snapshot.clone(),
        format!("{snapshot}:index"),
        format!("{snapshot}:seq"),
    ] {
        let ttl: i64 = redis::cmd("PTTL")
            .arg(key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(
            ttl > 0 && ttl <= 2_000,
            "snapshot TTL was not aligned: {ttl}"
        );
    }

    tokio::time::sleep(std::time::Duration::from_millis(2_200)).await;
    assert!(store.list_since(run_id, None, 1).await.unwrap().is_empty());
    assert!(
        !redis::cmd("EXISTS")
            .arg(&migration)
            .query_async::<bool>(&mut conn)
            .await
            .unwrap()
    );

    let replacement = event(run_id, "replacement-after-expiry");
    store.publish(replacement.clone()).await.unwrap();
    assert_eq!(
        store.list_since(run_id, None, 1).await.unwrap(),
        vec![replacement]
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_expired_owned_current_quarantine_ignores_an_unowned_raw_collision() {
    const EVENT_COUNT: usize = 5_000;
    let Some(fixture) = RedisTest::connect("event_legacy_expired_current_collision").await else {
        return;
    };
    let run_id = ":";
    let current = format!("{}events:~3a", fixture.prefix);
    let raw = format!("{}events:{run_id}", fixture.prefix);
    let raw_sequence = format!("{raw}:seq");
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy(&mut conn, &current, run_id, EVENT_COUNT).await;
    redis::pipe()
        .cmd("HSET")
        .arg(format!("{current}:meta"))
        .arg("run_id")
        .arg(run_id)
        .cmd("SET")
        .arg(&raw_sequence)
        .arg(0)
        .cmd("PEXPIRE")
        .arg(&current)
        .arg(1_500)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();
    let store = fixture.event_store(None).await;
    assert_eq!(
        store.list_since(run_id, None, 1).await.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );

    tokio::time::sleep(std::time::Duration::from_millis(1_700)).await;
    assert!(store.list_since(run_id, None, 1).await.unwrap().is_empty());
    assert_eq!(
        redis::cmd("GET")
            .arg(&raw_sequence)
            .query_async::<String>(&mut conn)
            .await
            .unwrap(),
        "0"
    );
    let replacement = event(run_id, "replacement-after-owned-current-expiry");
    store.publish(replacement.clone()).await.unwrap();
    assert_eq!(
        store.list_since(run_id, None, 1).await.unwrap(),
        vec![replacement]
    );
    assert_eq!(
        redis::cmd("GET")
            .arg(&raw_sequence)
            .query_async::<String>(&mut conn)
            .await
            .unwrap(),
        "0"
    );
    fixture.cleanup().await;
}
