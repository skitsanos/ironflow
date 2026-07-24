use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::StorageErrorKind;
use ironflow::storage::event_store::{EventStore, RedisEventStore};

use super::redis_support::RedisTest;

const LEGACY_MAX_EVENT_BYTES: usize = 1_048_576;

fn event(run_id: &str, id: String) -> RunEvent {
    let mut event = RunEvent::run(
        run_id,
        "legacy-limits",
        RunEventType::ContextUpdated,
        RunStatus::Running,
    );
    event.id = id;
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
        let end = (start + 400).min(count);
        let mut pipe = redis::pipe();
        for position in start..end {
            let stored = event(run_id, format!("legacy-limit-{position:05}"));
            pipe.cmd("RPUSH")
                .arg(base)
                .arg(serde_json::to_string(&stored).unwrap())
                .cmd("HSET")
                .arg(&index)
                .arg(&stored.id)
                .arg(position + 1);
        }
        let _: () = pipe.query_async(conn).await.unwrap();
    }
    let _: () = redis::cmd("SET")
        .arg(format!("{base}:seq"))
        .arg(count)
        .query_async(conn)
        .await
        .unwrap();
}

async fn retry_listing(store: &RedisEventStore, run_id: &str) -> Vec<RunEvent> {
    for _ in 0..16 {
        match store.list_since(run_id, None, 1).await {
            Ok(events) => return events,
            Err(error)
                if error.kind() == StorageErrorKind::Conflict
                    && error.diagnostic().contains("migration") =>
            {
                tokio::task::yield_now().await;
            }
            Err(error) => panic!("legacy migration failed: {error}"),
        }
    }
    panic!("legacy migration did not finish within its bounded retries")
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

async fn read_family(
    conn: &mut redis::aio::ConnectionManager,
    base: &str,
) -> (
    Vec<String>,
    HashMap<String, String>,
    Option<String>,
    HashMap<String, String>,
) {
    redis::pipe()
        .cmd("LRANGE")
        .arg(base)
        .arg(0)
        .arg(-1)
        .cmd("HGETALL")
        .arg(format!("{base}:index"))
        .cmd("GET")
        .arg(format!("{base}:seq"))
        .cmd("HGETALL")
        .arg(format!("{base}:meta"))
        .query_async(conn)
        .await
        .unwrap()
}

#[tokio::test]
async fn redis_legacy_migration_rejects_an_oversized_event_without_moving_it() {
    let Some(fixture) = RedisTest::connect("event_legacy_oversized").await else {
        return;
    };
    let run_id = "legacy-oversized-event";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let mut stored = event(run_id, "legacy-oversized-id".to_string());
    stored.reason = Some("x".repeat(LEGACY_MAX_EVENT_BYTES));
    let raw = serde_json::to_string(&stored).unwrap();
    assert!(raw.len() > LEGACY_MAX_EVENT_BYTES);

    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::pipe()
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
        .query_async(&mut conn)
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
    assert!(marker.is_none());
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_legacy_migration_respects_the_aggregate_batch_byte_limit() {
    const EVENT_COUNT: usize = 24;
    const PAYLOAD_BYTES: usize = 100_000;
    let Some(fixture) = RedisTest::connect("event_legacy_batch_bytes").await else {
        return;
    };
    let run_id = "legacy-batch-bytes";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let index = format!("{base}:index");
    let mut conn = fixture.connection().await.unwrap();
    let mut expected = Vec::with_capacity(EVENT_COUNT);
    let mut pipe = redis::pipe();
    for position in 0..EVENT_COUNT {
        let mut stored = event(run_id, format!("legacy-byte-{position:02}"));
        stored.reason = Some("x".repeat(PAYLOAD_BYTES));
        expected.push(stored.clone());
        pipe.cmd("RPUSH")
            .arg(&base)
            .arg(serde_json::to_string(&stored).unwrap())
            .cmd("HSET")
            .arg(&index)
            .arg(&stored.id)
            .arg(position + 1);
    }
    pipe.cmd("SET")
        .arg(format!("{base}:seq"))
        .arg(EVENT_COUNT)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();

    let actual = fixture
        .event_store(None)
        .await
        .list_since(run_id, None, EVENT_COUNT)
        .await
        .unwrap();
    assert_eq!(actual, expected);
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_legacy_migration_never_refreshes_source_ttl() {
    const EVENT_COUNT: usize = 4_200;
    let Some(fixture) = RedisTest::connect("event_legacy_ttl").await else {
        return;
    };
    let run_id = "legacy-event-ttl";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy(&mut conn, &base, run_id, EVENT_COUNT).await;
    let family = [base.clone(), format!("{base}:index"), format!("{base}:seq")];
    for key in &family {
        let changed: bool = redis::cmd("PEXPIRE")
            .arg(key)
            .arg(60_000)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(changed);
    }
    let initial_ttl: i64 = redis::cmd("PTTL")
        .arg(&base)
        .query_async(&mut conn)
        .await
        .unwrap();

    let store = fixture.event_store(Some(120)).await;
    let first = store.list_since(run_id, None, 1).await.unwrap_err();
    assert_eq!(first.kind(), StorageErrorKind::Conflict);
    let snapshot = format!(
        "{}event_migrations:v1:{}:snapshot",
        fixture.prefix,
        hex::encode(run_id)
    );
    let after_checkpoint: i64 = redis::cmd("PTTL")
        .arg(&snapshot)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(after_checkpoint > 0 && after_checkpoint <= initial_ttl);

    assert_eq!(retry_listing(&store, run_id).await.len(), 1);
    let current_family = [
        base.clone(),
        format!("{base}:index"),
        format!("{base}:seq"),
        format!("{base}:meta"),
    ];
    let mut final_ttls = Vec::new();
    for key in &current_family {
        final_ttls.push(
            redis::cmd("PTTL")
                .arg(key)
                .query_async::<i64>(&mut conn)
                .await
                .unwrap(),
        );
    }
    assert!(
        final_ttls.iter().all(|ttl| *ttl > 0 && *ttl <= initial_ttl),
        "migration refreshed a legacy TTL: {final_ttls:?}"
    );
    let spread = final_ttls.iter().max().unwrap() - final_ttls.iter().min().unwrap();
    assert!(
        spread <= 100,
        "adopted family TTLs diverged: {final_ttls:?}"
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_legacy_finalize_acl_denial_preserves_quarantine_and_can_resume() {
    const EVENT_COUNT: usize = 4_096;
    let Some(fixture) = RedisTest::connect("event_legacy_finalize_acl").await else {
        return;
    };
    let run_id = "legacy-finalize-acl";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let state = format!(
        "{}event_migrations:v1:{}",
        fixture.prefix,
        hex::encode(run_id)
    );
    let mut admin = fixture.connection().await.unwrap();
    seed_legacy(&mut admin, &base, run_id, EVENT_COUNT).await;

    let admin_store = fixture.event_store(None).await;
    assert_eq!(
        admin_store
            .list_since(run_id, None, 1)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );
    let (phase, cursor): (String, usize) = redis::pipe()
        .cmd("HGET")
        .arg(&state)
        .arg("phase")
        .cmd("HGET")
        .arg(&state)
        .arg("cursor")
        .query_async(&mut admin)
        .await
        .unwrap();
    assert_eq!((phase.as_str(), cursor), ("verify", 0));
    let frozen = format!("{state}:snapshot");
    let before = read_family(&mut admin, &frozen).await;

    let identity = uuid::Uuid::new_v4().simple().to_string();
    let username = format!("ironflow_test_{identity}");
    let password = format!("secret_{identity}");
    let _: () = redis::cmd("ACL")
        .arg("SETUSER")
        .arg(&username)
        .arg("reset")
        .arg("on")
        .arg(format!(">{password}"))
        .arg(format!("~{}*", fixture.prefix))
        .arg("+@all")
        .arg("-rename")
        .query_async(&mut admin)
        .await
        .unwrap();
    let mut restricted_url = url::Url::parse(&fixture.url).unwrap();
    restricted_url.set_username(&username).unwrap();
    restricted_url.set_password(Some(&password)).unwrap();
    let restricted =
        RedisEventStore::new(restricted_url.as_str(), Some(fixture.prefix.clone()), None)
            .await
            .unwrap();
    let error = restricted.list_since(run_id, None, 1).await.unwrap_err();
    drop(restricted);
    assert_eq!(error.kind(), StorageErrorKind::Backend);
    assert_eq!(read_family(&mut admin, &frozen).await, before);
    let current_exists: usize = redis::cmd("EXISTS")
        .arg(&base)
        .arg(format!("{base}:index"))
        .arg(format!("{base}:seq"))
        .arg(format!("{base}:meta"))
        .query_async(&mut admin)
        .await
        .unwrap();
    assert_eq!(current_exists, 0);

    let deleted_users: usize = redis::cmd("ACL")
        .arg("DELUSER")
        .arg(&username)
        .query_async(&mut admin)
        .await
        .unwrap();
    assert_eq!(deleted_users, 1);
    assert_eq!(retry_listing(&admin_store, run_id).await.len(), 1);
    let layout: String = redis::cmd("HGET")
        .arg(format!("{base}:meta"))
        .arg("layout")
        .query_async(&mut admin)
        .await
        .unwrap();
    assert_eq!(layout, "2");
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_legacy_acl_preflight_never_leaks_probe_keys() {
    let Some(fixture) = RedisTest::connect("event_legacy_probe_acl").await else {
        return;
    };
    let run_id = "legacy-probe-acl";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let mut admin = fixture.connection().await.unwrap();
    seed_legacy(&mut admin, &base, run_id, 1).await;
    let before = read_family(&mut admin, &base).await;

    let identity = uuid::Uuid::new_v4().simple().to_string();
    let username = format!("ironflow_test_{identity}");
    let password = format!("secret_{identity}");
    redis::cmd("ACL")
        .arg("SETUSER")
        .arg(&username)
        .arg("reset")
        .arg("on")
        .arg(format!(">{password}"))
        .arg(format!("~{}*", fixture.prefix))
        .arg("+@all")
        .arg("-persist")
        .query_async::<()>(&mut admin)
        .await
        .unwrap();
    let mut restricted_url = url::Url::parse(&fixture.url).unwrap();
    restricted_url.set_username(&username).unwrap();
    restricted_url.set_password(Some(&password)).unwrap();
    let restricted =
        RedisEventStore::new(restricted_url.as_str(), Some(fixture.prefix.clone()), None)
            .await
            .unwrap();
    let error = restricted.list_since(run_id, None, 1).await.unwrap_err();
    drop(restricted);
    assert_eq!(error.kind(), StorageErrorKind::Backend);
    assert_eq!(read_family(&mut admin, &base).await, before);
    let probes: Vec<String> = redis::cmd("KEYS")
        .arg(format!("{}event_migration_probes:v1:*", fixture.prefix))
        .query_async(&mut admin)
        .await
        .unwrap();
    assert!(probes.is_empty(), "ACL preflight leaked keys: {probes:?}");

    redis::cmd("ACL")
        .arg("DELUSER")
        .arg(&username)
        .query_async::<usize>(&mut admin)
        .await
        .unwrap();
    assert_eq!(
        retry_listing(&fixture.event_store(None).await, run_id)
            .await
            .len(),
        1
    );
    fixture.cleanup().await;
}
use std::collections::HashMap;
