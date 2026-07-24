use std::sync::Arc;

use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::StorageErrorKind;
use ironflow::storage::event_store::{EventStore, RedisEventStore};
use tokio::sync::Barrier;
use tokio::task::JoinSet;

use super::redis_support::RedisTest;

fn event(run_id: &str, id: &str) -> RunEvent {
    let mut event = RunEvent::run(
        run_id,
        "event-delete-flow",
        RunEventType::ContextUpdated,
        RunStatus::Running,
    );
    event.id = id.to_string();
    event
}

fn event_keys(prefix: &str, segment: &str) -> [String; 4] {
    let base = format!("{prefix}events:{segment}");
    [
        base.clone(),
        format!("{base}:index"),
        format!("{base}:seq"),
        format!("{base}:meta"),
    ]
}

fn deletion_fence_key(prefix: &str, segment: &str) -> String {
    format!("{prefix}event_deletions:v1:{segment}")
}

#[tokio::test]
async fn redis_event_delete_is_atomic_counted_idempotent_and_ttl_safe() {
    let Some(fixture) = RedisTest::connect("event_delete").await else {
        return;
    };
    let store = fixture.event_store(Some(60)).await;
    let run_id = "event-delete-run";
    let first = event(run_id, "event-delete-1");
    let second = event(run_id, "event-delete-2");
    store.publish(first.clone()).await.unwrap();
    store.publish(second).await.unwrap();

    let keys = event_keys(&fixture.prefix, run_id);
    let mut conn = fixture.connection().await.unwrap();
    for key in &keys {
        let ttl: i64 = redis::cmd("TTL")
            .arg(key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!((1..=60).contains(&ttl));
    }

    assert_eq!(store.delete_run(run_id).await.unwrap(), 2);
    let exists: usize = redis::cmd("EXISTS")
        .arg(&keys)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(exists, 0, "event deletion left part of the key family");
    let fence = deletion_fence_key(&fixture.prefix, run_id);
    let (owner, ttl): (String, i64) = redis::pipe()
        .cmd("GET")
        .arg(&fence)
        .cmd("TTL")
        .arg(&fence)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(owner, run_id);
    assert!((1..=60).contains(&ttl));
    assert_eq!(
        store
            .publish(event(run_id, "event-delete-late"))
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );
    let exists: usize = redis::cmd("EXISTS")
        .arg(&keys)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(exists, 0, "a late publisher recreated deleted events");
    let fence_ttl_before_retry: i64 = redis::cmd("PTTL")
        .arg(&fence)
        .query_async(&mut conn)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    assert_eq!(store.delete_run(run_id).await.unwrap(), 0);
    let fence_ttl_after_retry: i64 = redis::cmd("PTTL")
        .arg(&fence)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(
        fence_ttl_after_retry > 0 && fence_ttl_after_retry < fence_ttl_before_retry,
        "an idempotent delete retry refreshed the original fence lifetime"
    );
    assert_eq!(
        store
            .list_since(run_id, Some(&first.id), 1)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );

    let expired_run = "event-delete-expired";
    let expiring_store = fixture.event_store(Some(1)).await;
    expiring_store
        .publish(event(expired_run, "event-delete-expiring"))
        .await
        .unwrap();
    let expired_keys = event_keys(&fixture.prefix, expired_run);
    for _ in 0..60 {
        let exists: usize = redis::cmd("EXISTS")
            .arg(&expired_keys)
            .query_async(&mut conn)
            .await
            .unwrap();
        if exists == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(expiring_store.delete_run(expired_run).await.unwrap(), 0);
    let expired_fence = deletion_fence_key(&fixture.prefix, expired_run);
    let ttl: i64 = redis::cmd("TTL")
        .arg(&expired_fence)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!((1..=1).contains(&ttl));
    for _ in 0..60 {
        let exists: bool = redis::cmd("EXISTS")
            .arg(&expired_fence)
            .query_async(&mut conn)
            .await
            .unwrap();
        if !exists {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let fence_exists: bool = redis::cmd("EXISTS")
        .arg(&expired_fence)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!fence_exists, "configured deletion fence did not expire");
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_event_delete_migrates_and_removes_an_alias_safe_legacy_family() {
    let Some(fixture) = RedisTest::connect("event_delete_legacy").await else {
        return;
    };
    let run_id = "legacy:event-delete";
    let stored = event(run_id, "legacy-delete-event");
    let raw_keys = event_keys(&fixture.prefix, run_id);
    let encoded_segment = format!("~{}", hex::encode(run_id.as_bytes()));
    let encoded_keys = event_keys(&fixture.prefix, &encoded_segment);
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::pipe()
        .cmd("RPUSH")
        .arg(&raw_keys[0])
        .arg(serde_json::to_string(&stored).unwrap())
        .cmd("HSET")
        .arg(&raw_keys[1])
        .arg(&stored.id)
        .arg(1_u8)
        .cmd("SET")
        .arg(&raw_keys[2])
        .arg(1_u8)
        .cmd("HSET")
        .arg(&raw_keys[3])
        .arg("layout")
        .arg("2")
        .arg("run_id")
        .arg(run_id)
        .query_async(&mut conn)
        .await
        .unwrap();

    let store = fixture.event_store(None).await;
    assert_eq!(store.delete_run(run_id).await.unwrap(), 1);
    let exists: usize = redis::cmd("EXISTS")
        .arg(
            raw_keys
                .iter()
                .chain(encoded_keys.iter())
                .collect::<Vec<_>>(),
        )
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(exists, 0);
    let fence = deletion_fence_key(&fixture.prefix, &encoded_segment);
    let (owner, ttl): (String, i64) = redis::pipe()
        .cmd("GET")
        .arg(&fence)
        .cmd("TTL")
        .arg(&fence)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(owner, run_id);
    assert_eq!(ttl, -1, "an unconfigured deletion fence must persist");
    assert_eq!(store.delete_run(run_id).await.unwrap(), 0);
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_event_delete_faults_before_removing_any_valid_key() {
    let Some(fixture) = RedisTest::connect("event_delete_fault").await else {
        return;
    };
    let run_id = "event-delete-fault";
    let store = fixture.event_store(None).await;
    store
        .publish(event(run_id, "event-delete-fault-event"))
        .await
        .unwrap();

    let keys = event_keys(&fixture.prefix, run_id);
    let fence = deletion_fence_key(&fixture.prefix, run_id);
    let mut conn = fixture.connection().await.unwrap();

    let arity_result: redis::RedisResult<usize> = redis::Script::new(include_str!(
        "../../src/storage/event_store/scripts/delete.lua"
    ))
    .key(&keys[0])
    .key(&keys[1])
    .key(&keys[2])
    .key(&keys[3])
    .arg(run_id)
    .arg(-1)
    .invoke_async(&mut conn)
    .await;
    assert!(arity_result.is_err());
    let exists: usize = redis::cmd("EXISTS")
        .arg(&keys)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(exists, 4, "invalid script arity removed event keys");

    let _: () = redis::pipe()
        .cmd("DEL")
        .arg(&keys[1])
        .cmd("SET")
        .arg(&keys[1])
        .arg("wrong-type")
        .query_async(&mut conn)
        .await
        .unwrap();

    assert_eq!(
        store.delete_run(run_id).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
    let exists: usize = redis::cmd("EXISTS")
        .arg(&keys)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(exists, 4, "failed deletion partially removed event keys");
    let fence_exists: bool = redis::cmd("EXISTS")
        .arg(&fence)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!fence_exists, "failed deletion installed a fence");

    let fence_fault_run = "event-delete-fence-fault";
    let fence_fault_keys = event_keys(&fixture.prefix, fence_fault_run);
    let fence_fault = deletion_fence_key(&fixture.prefix, fence_fault_run);
    store
        .publish(event(fence_fault_run, "event-delete-fence-fault-event"))
        .await
        .unwrap();
    let _: usize = redis::cmd("RPUSH")
        .arg(&fence_fault)
        .arg("wrong-type")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        store.delete_run(fence_fault_run).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
    assert_eq!(
        store
            .publish(event(fence_fault_run, "event-after-fence-fault"))
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    let exists: usize = redis::cmd("EXISTS")
        .arg(&fence_fault_keys)
        .arg(&fence_fault)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(exists, 5, "invalid fence handling mutated the stream");
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_event_delete_unlink_acl_denial_does_not_install_a_fence() {
    let Some(fixture) = RedisTest::connect("event_delete_unlink_acl").await else {
        return;
    };
    let run_id = "event-delete-unlink-acl";
    let admin_store = fixture.event_store(None).await;
    admin_store
        .publish(event(run_id, "event-delete-unlink-acl-event"))
        .await
        .unwrap();

    let keys = event_keys(&fixture.prefix, run_id);
    let fence = deletion_fence_key(&fixture.prefix, run_id);
    let mut admin = fixture.connection().await.unwrap();
    let before: Vec<Vec<u8>> = redis::pipe()
        .cmd("DUMP")
        .arg(&keys[0])
        .cmd("DUMP")
        .arg(&keys[1])
        .cmd("DUMP")
        .arg(&keys[2])
        .cmd("DUMP")
        .arg(&keys[3])
        .query_async(&mut admin)
        .await
        .unwrap();

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
        .arg("-unlink")
        .query_async(&mut admin)
        .await
        .unwrap();

    let mut restricted_url = url::Url::parse(&fixture.url).unwrap();
    restricted_url.set_username(&username).unwrap();
    restricted_url.set_password(Some(&password)).unwrap();
    let restricted_store =
        RedisEventStore::new(restricted_url.as_str(), Some(fixture.prefix.clone()), None)
            .await
            .unwrap();
    let delete_error = restricted_store.delete_run(run_id).await.unwrap_err();
    drop(restricted_store);

    let after: Vec<Vec<u8>> = redis::pipe()
        .cmd("DUMP")
        .arg(&keys[0])
        .cmd("DUMP")
        .arg(&keys[1])
        .cmd("DUMP")
        .arg(&keys[2])
        .cmd("DUMP")
        .arg(&keys[3])
        .query_async(&mut admin)
        .await
        .unwrap();
    let fence_exists: bool = redis::cmd("EXISTS")
        .arg(&fence)
        .query_async(&mut admin)
        .await
        .unwrap();
    let probe_keys: Vec<String> = redis::cmd("KEYS")
        .arg(format!("{}event_delete_probes:v1:*", fixture.prefix))
        .query_async(&mut admin)
        .await
        .unwrap();
    let deleted_users: usize = redis::cmd("ACL")
        .arg("DELUSER")
        .arg(&username)
        .query_async(&mut admin)
        .await
        .unwrap();
    fixture.cleanup().await;

    assert_eq!(delete_error.kind(), StorageErrorKind::Backend);
    assert_eq!(before, after, "denied UNLINK mutated the event namespace");
    assert!(!fence_exists, "denied UNLINK installed a deletion fence");
    assert!(
        probe_keys.is_empty(),
        "UNLINK probe key was unexpectedly created"
    );
    assert_eq!(deleted_users, 1, "temporary ACL user was not removed");
}

#[tokio::test]
async fn redis_event_delete_installs_ttl_fence_with_one_set_command() {
    let Some(fixture) = RedisTest::connect("event_delete_fence_set_acl").await else {
        return;
    };
    let run_id = "event-delete-fence-set-acl";
    let admin_store = fixture.event_store(Some(60)).await;
    admin_store
        .publish(event(run_id, "event-delete-fence-set-acl-event"))
        .await
        .unwrap();

    let identity = uuid::Uuid::new_v4().simple().to_string();
    let username = format!("ironflow_test_{identity}");
    let password = format!("secret_{identity}");
    let mut admin = fixture.connection().await.unwrap();
    let _: () = redis::cmd("ACL")
        .arg("SETUSER")
        .arg(&username)
        .arg("reset")
        .arg("on")
        .arg(format!(">{password}"))
        .arg(format!("~{}*", fixture.prefix))
        .arg("+@all")
        .arg("-expire")
        .arg("-persist")
        .query_async(&mut admin)
        .await
        .unwrap();

    let mut restricted_url = url::Url::parse(&fixture.url).unwrap();
    restricted_url.set_username(&username).unwrap();
    restricted_url.set_password(Some(&password)).unwrap();
    let restricted_store = RedisEventStore::new(
        restricted_url.as_str(),
        Some(fixture.prefix.clone()),
        Some(60),
    )
    .await
    .unwrap();
    assert_eq!(restricted_store.delete_run(run_id).await.unwrap(), 1);
    drop(restricted_store);

    let keys = event_keys(&fixture.prefix, run_id);
    let fence = deletion_fence_key(&fixture.prefix, run_id);
    let (stream_keys, owner, ttl): (usize, String, i64) = redis::pipe()
        .cmd("EXISTS")
        .arg(&keys)
        .cmd("GET")
        .arg(&fence)
        .cmd("TTL")
        .arg(&fence)
        .query_async(&mut admin)
        .await
        .unwrap();
    let deleted_users: usize = redis::cmd("ACL")
        .arg("DELUSER")
        .arg(&username)
        .query_async(&mut admin)
        .await
        .unwrap();
    fixture.cleanup().await;

    assert_eq!(stream_keys, 0);
    assert_eq!(owner, run_id);
    assert!((1..=60).contains(&ttl));
    assert_eq!(deleted_users, 1, "temporary ACL user was not removed");
}

#[tokio::test]
async fn redis_event_delete_fences_publishers_on_both_sides_of_the_race() {
    const PUBLISHERS: usize = 16;

    let Some(fixture) = RedisTest::connect("event_delete_publish_race").await else {
        return;
    };
    let run_id = "event-delete-race";
    let store = fixture.event_store(None).await;
    store
        .publish(event(run_id, "event-delete-seed"))
        .await
        .unwrap();

    let barrier = Arc::new(Barrier::new(PUBLISHERS + 2));
    let mut publishers = JoinSet::new();
    for index in 0..PUBLISHERS {
        let writer = fixture.event_store(None).await;
        let barrier = barrier.clone();
        publishers.spawn(async move {
            barrier.wait().await;
            writer
                .publish(event(run_id, &format!("event-delete-racer-{index:02}")))
                .await
        });
    }
    let deleter = fixture.event_store(None).await;
    let delete_barrier = barrier.clone();
    let deletion = tokio::spawn(async move {
        delete_barrier.wait().await;
        deleter.delete_run(run_id).await
    });

    barrier.wait().await;
    let mut published_before_delete = 0;
    while let Some(result) = publishers.join_next().await {
        match result.unwrap() {
            Ok(()) => published_before_delete += 1,
            Err(error) => assert_eq!(error.kind(), StorageErrorKind::Conflict),
        }
    }
    let deleted = deletion.await.unwrap().unwrap();
    assert_eq!(deleted, published_before_delete + 1);

    let mut conn = fixture.connection().await.unwrap();
    let keys = event_keys(&fixture.prefix, run_id);
    let fence = deletion_fence_key(&fixture.prefix, run_id);
    let (stream_keys, fence_owner, fence_ttl): (usize, String, i64) = redis::pipe()
        .cmd("EXISTS")
        .arg(&keys)
        .cmd("GET")
        .arg(&fence)
        .cmd("TTL")
        .arg(&fence)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(stream_keys, 0);
    assert_eq!(fence_owner, run_id);
    assert_eq!(fence_ttl, -1);
    assert_eq!(
        store
            .publish(event(run_id, "event-delete-post-race"))
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );
    fixture.cleanup().await;
}
