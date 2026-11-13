use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::StorageErrorKind;
use ironflow::storage::event_store::EventStore;

use std::time::{Duration, Instant};

use super::redis_support::RedisTest;

/// Expiries a deadline scenario arms, shortest first. The steps that must
/// finish before the expiry are timed, and a scenario that could not finish
/// them with a quarter of the expiry to spare is repeated with the next
/// expiry instead of being asserted against an already expired stream.
const EXPIRY_LADDER_MS: [u64; 3] = [2_000, 8_000, 32_000];
const EXPIRY_POLL: Duration = Duration::from_millis(20);
const EXPIRY_POLL_LIMIT: Duration = Duration::from_secs(60);

enum Setup {
    Complete,
    OutranExpiry,
}

/// Whether the steps since `armed` left less than a quarter of the expiry.
/// `armed` is taken before the expiry is sent, so it never under-reports.
fn outran_expiry(armed: Instant, expiry_ms: u64) -> bool {
    let elapsed = armed.elapsed();
    let outran = elapsed >= Duration::from_millis(expiry_ms - expiry_ms / 4);
    if outran {
        eprintln!("setup took {elapsed:?} of a {expiry_ms} ms expiry; repeating the scenario");
    }
    outran
}

async fn wait_until_absent(conn: &mut redis::aio::ConnectionManager, keys: &[String]) {
    let limit = Instant::now() + EXPIRY_POLL_LIMIT;
    loop {
        let present: usize = redis::cmd("EXISTS")
            .arg(keys)
            .query_async(conn)
            .await
            .unwrap();
        if present == 0 {
            return;
        }
        assert!(
            Instant::now() < limit,
            "{present} of {keys:?} did not expire within {EXPIRY_POLL_LIMIT:?}"
        );
        tokio::time::sleep(EXPIRY_POLL).await;
    }
}

fn snapshot_family(snapshot: &str) -> [String; 4] {
    [
        snapshot.to_string(),
        format!("{snapshot}:index"),
        format!("{snapshot}:seq"),
        format!("{snapshot}:meta"),
    ]
}

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
    for expiry_ms in EXPIRY_LADDER_MS {
        if matches!(hostile_persist_scenario(expiry_ms).await, Setup::Complete) {
            return;
        }
    }
    panic!("setup outran every expiry in {EXPIRY_LADDER_MS:?}");
}

async fn hostile_persist_scenario(expiry_ms: u64) -> Setup {
    const EVENT_COUNT: usize = 4_200;
    let Some(fixture) = RedisTest::connect("event_legacy_restore_expired_ttl").await else {
        return Setup::Complete;
    };
    let run_id = "legacy-restore-expired-ttl";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy(&mut conn, &base, run_id, EVENT_COUNT).await;
    let store = fixture.event_store(None).await;
    let migration = migration_key(&fixture.prefix, run_id);
    let snapshot = format!("{migration}:snapshot");
    let frozen = snapshot_family(&snapshot);

    // Everything from here to the `outran_expiry` check must happen before
    // the expiry, so the results are collected first and asserted afterwards.
    let armed = Instant::now();
    let mut pipe = redis::pipe();
    for key in [&base, &format!("{base}:index"), &format!("{base}:seq")] {
        pipe.cmd("PEXPIRE").arg(key).arg(expiry_ms);
    }
    let expiring_keys: Vec<bool> = pipe.query_async(&mut conn).await.unwrap();
    let frozen_result = store.list_since(run_id, None, 1).await;
    let remaining: i64 = redis::cmd("PTTL")
        .arg(&snapshot)
        .query_async(&mut conn)
        .await
        .unwrap();
    let mut pipe = redis::pipe();
    for key in &frozen[..3] {
        pipe.cmd("PERSIST").arg(key);
    }
    let persisted: Vec<bool> = pipe.query_async(&mut conn).await.unwrap();
    let head: Option<String> = redis::cmd("LINDEX")
        .arg(&snapshot)
        .arg(0)
        .query_async(&mut conn)
        .await
        .unwrap();
    if let Some(head) = &head {
        let head: RunEvent = serde_json::from_str(head).unwrap();
        redis::cmd("HSET")
            .arg(&frozen[1])
            .arg(head.id)
            .arg(1)
            .query_async::<()>(&mut conn)
            .await
            .unwrap();
    }
    if outran_expiry(armed, expiry_ms) {
        fixture.cleanup().await;
        return Setup::OutranExpiry;
    }
    assert_eq!(expiring_keys, [true; 3]);
    assert_eq!(
        frozen_result.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );
    assert!(
        remaining > 0 && remaining <= expiry_ms as i64,
        "snapshot TTL was not aligned: {remaining}"
    );
    assert_eq!(persisted, [true; 3]);
    assert!(head.is_some(), "the snapshot lost its first event");

    // The hostile PERSIST removed every Redis expiry, so only the captured
    // deadline can end the stream: wait out the TTL observed before it.
    tokio::time::sleep(Duration::from_millis(remaining as u64 + 100)).await;
    let expiring = store.list_since(run_id, None, 1).await.unwrap_err();
    assert_eq!(expiring.kind(), StorageErrorKind::Conflict);
    assert!(expiring.diagnostic().contains("expiring"));
    wait_until_absent(&mut conn, &frozen).await;
    assert!(store.list_since(run_id, None, 1).await.unwrap().is_empty());
    assert_eq!(
        redis::cmd("EXISTS")
            .arg(&base)
            .arg(format!("{base}:index"))
            .arg(format!("{base}:seq"))
            .arg(&migration)
            .arg(&frozen)
            .query_async::<usize>(&mut conn)
            .await
            .unwrap(),
        0
    );
    fixture.cleanup().await;
    Setup::Complete
}

#[tokio::test]
async fn redis_expired_legacy_quarantine_releases_its_persistent_state() {
    for expiry_ms in EXPIRY_LADDER_MS {
        if matches!(
            expired_quarantine_scenario(expiry_ms).await,
            Setup::Complete
        ) {
            return;
        }
    }
    panic!("setup outran every expiry in {EXPIRY_LADDER_MS:?}");
}

async fn expired_quarantine_scenario(expiry_ms: u64) -> Setup {
    const EVENT_COUNT: usize = 10_000;
    let Some(fixture) = RedisTest::connect("event_legacy_expired_quarantine").await else {
        return Setup::Complete;
    };
    let run_id = "legacy-expired-quarantine";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy(&mut conn, &base, run_id, EVENT_COUNT).await;
    let store = fixture.event_store(None).await;
    let migration = migration_key(&fixture.prefix, run_id);
    let frozen = snapshot_family(&format!("{migration}:snapshot"));

    let armed = Instant::now();
    let expiring_key: bool = redis::cmd("PEXPIRE")
        .arg(&base)
        .arg(expiry_ms)
        .query_async(&mut conn)
        .await
        .unwrap();
    let frozen_result = store.list_since(run_id, None, 1).await;
    let mut pipe = redis::pipe();
    for key in &frozen[..3] {
        pipe.cmd("PTTL").arg(key);
    }
    let aligned: Vec<i64> = pipe.query_async(&mut conn).await.unwrap();
    if outran_expiry(armed, expiry_ms) {
        fixture.cleanup().await;
        return Setup::OutranExpiry;
    }
    assert!(expiring_key);
    assert_eq!(
        frozen_result.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );
    assert!(
        aligned
            .iter()
            .all(|ttl| *ttl > 0 && *ttl <= expiry_ms as i64),
        "snapshot TTL was not aligned: {aligned:?}"
    );

    wait_until_absent(&mut conn, &frozen).await;
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
    Setup::Complete
}

#[tokio::test]
async fn redis_expired_owned_current_quarantine_ignores_an_unowned_raw_collision() {
    for expiry_ms in EXPIRY_LADDER_MS {
        if matches!(
            expired_current_collision_scenario(expiry_ms).await,
            Setup::Complete
        ) {
            return;
        }
    }
    panic!("setup outran every expiry in {EXPIRY_LADDER_MS:?}");
}

async fn expired_current_collision_scenario(expiry_ms: u64) -> Setup {
    const EVENT_COUNT: usize = 5_000;
    let Some(fixture) = RedisTest::connect("event_legacy_expired_current_collision").await else {
        return Setup::Complete;
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
        .query_async::<()>(&mut conn)
        .await
        .unwrap();
    let store = fixture.event_store(None).await;
    let migration = migration_key(&fixture.prefix, run_id);
    let frozen = snapshot_family(&format!("{migration}:snapshot"));

    let armed = Instant::now();
    let expiring_key: bool = redis::cmd("PEXPIRE")
        .arg(&current)
        .arg(expiry_ms)
        .query_async(&mut conn)
        .await
        .unwrap();
    let frozen_result = store.list_since(run_id, None, 1).await;
    let remaining: i64 = redis::cmd("PTTL")
        .arg(&frozen[0])
        .query_async(&mut conn)
        .await
        .unwrap();
    if outran_expiry(armed, expiry_ms) {
        fixture.cleanup().await;
        return Setup::OutranExpiry;
    }
    assert!(expiring_key);
    assert_eq!(
        frozen_result.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );
    assert!(
        remaining > 0 && remaining <= expiry_ms as i64,
        "snapshot TTL was not aligned: {remaining}"
    );

    wait_until_absent(&mut conn, &frozen).await;
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
    Setup::Complete
}
