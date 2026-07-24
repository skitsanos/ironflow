use std::sync::Arc;

use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::event_store::EventStore;
use ironflow::storage::{StorageErrorKind, event_store::RedisEventStore};
use tokio::sync::Barrier;
use tokio::task::JoinSet;

use super::redis_support::RedisTest;

const LEGACY_EVENTS: usize = 5_000;
const PUBLISHERS: usize = 8;

fn event(run_id: &str, id: String) -> RunEvent {
    let mut event = RunEvent::run(
        run_id,
        "legacy-race",
        RunEventType::ContextUpdated,
        RunStatus::Running,
    );
    event.id = id;
    event
}

async fn seed_legacy_events(
    conn: &mut redis::aio::ConnectionManager,
    base: &str,
    run_id: &str,
    count: usize,
) {
    let index = format!("{base}:index");
    for start in (0..count).step_by(250) {
        let end = (start + 250).min(count);
        let mut pipe = redis::pipe();
        for position in start..end {
            let stored = event(run_id, format!("legacy-race-{position:05}"));
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

async fn publish_until_settled(store: Arc<RedisEventStore>, event: RunEvent) -> bool {
    for _ in 0..64 {
        match store.publish(event.clone()).await {
            Ok(()) => return true,
            Err(error)
                if error.kind() == StorageErrorKind::Conflict
                    && error.diagnostic().contains("migration") =>
            {
                tokio::task::yield_now().await;
            }
            Err(error) if error.kind() == StorageErrorKind::Conflict => return false,
            Err(error) => panic!("legacy race publisher failed: {error}"),
        }
    }
    panic!("legacy race publisher exhausted its bounded retries")
}

async fn delete_until_complete(store: Arc<RedisEventStore>, run_id: &'static str) -> usize {
    for _ in 0..64 {
        match store.delete_run(run_id).await {
            Ok(deleted) => return deleted,
            Err(error)
                if error.kind() == StorageErrorKind::Conflict
                    && error.diagnostic().contains("migration") =>
            {
                tokio::task::yield_now().await;
            }
            Err(error) => panic!("legacy race deletion failed: {error}"),
        }
    }
    panic!("legacy race deletion exhausted its bounded retries")
}

#[tokio::test]
async fn redis_legacy_migration_publish_delete_race_is_fenced_without_resurrection() {
    let Some(fixture) = RedisTest::connect("event_legacy_publish_delete_race").await else {
        return;
    };
    const RUN_ID: &str = "legacy-event-race";
    let base = format!("{}events:{RUN_ID}", fixture.prefix);
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy_events(&mut conn, &base, RUN_ID, LEGACY_EVENTS).await;

    let store = Arc::new(fixture.event_store(None).await);
    let first = store.list_since(RUN_ID, None, 1).await.unwrap_err();
    assert_eq!(first.kind(), StorageErrorKind::Conflict);
    assert!(first.diagnostic().contains("bounded progress"));

    let barrier = Arc::new(Barrier::new(PUBLISHERS + 1));
    let mut writers = JoinSet::new();
    for index in 0..PUBLISHERS {
        let writer = Arc::clone(&store);
        let writer_barrier = Arc::clone(&barrier);
        writers.spawn(async move {
            writer_barrier.wait().await;
            publish_until_settled(writer, event(RUN_ID, format!("racing-{index}"))).await
        });
    }
    let deleter = Arc::clone(&store);
    let delete_barrier = Arc::clone(&barrier);
    let delete = tokio::spawn(async move {
        delete_barrier.wait().await;
        delete_until_complete(deleter, RUN_ID).await
    });

    let deleted = tokio::time::timeout(std::time::Duration::from_secs(30), delete)
        .await
        .expect("legacy race deletion timed out")
        .unwrap();
    let mut published = 0_usize;
    while let Some(result) = writers.join_next().await {
        published += usize::from(result.unwrap());
    }
    assert_eq!(deleted, LEGACY_EVENTS + published);

    let state = format!(
        "{}event_migrations:v1:{}",
        fixture.prefix,
        hex::encode(RUN_ID)
    );
    let fence = format!("{}event_deletions:v1:{RUN_ID}", fixture.prefix);
    let (event_keys, state_exists, fence_owner): (usize, bool, String) = redis::pipe()
        .cmd("EXISTS")
        .arg(&base)
        .arg(format!("{base}:index"))
        .arg(format!("{base}:seq"))
        .arg(format!("{base}:meta"))
        .cmd("EXISTS")
        .arg(&state)
        .cmd("GET")
        .arg(&fence)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(event_keys, 0);
    assert!(!state_exists);
    assert_eq!(fence_owner, RUN_ID);
    assert_eq!(
        store
            .publish(event(RUN_ID, "after-delete".to_string()))
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_legacy_source_recreation_preserves_both_namespaces() {
    let Some(fixture) = RedisTest::connect("event_legacy_source_recreated").await else {
        return;
    };
    let run_id = "legacy-source-recreated";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let snapshot = format!(
        "{}event_migrations:v1:{}:snapshot",
        fixture.prefix,
        hex::encode(run_id)
    );
    let mut conn = fixture.connection().await.unwrap();
    seed_legacy_events(&mut conn, &base, run_id, LEGACY_EVENTS).await;
    let store = fixture.event_store(None).await;
    assert_eq!(
        store.list_since(run_id, None, 1).await.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );

    let late = event(run_id, "pre-protocol-late-writer".to_string());
    let late_raw = serde_json::to_string(&late).unwrap();
    redis::pipe()
        .cmd("RPUSH")
        .arg(&base)
        .arg(&late_raw)
        .cmd("HSET")
        .arg(format!("{base}:index"))
        .arg(&late.id)
        .arg(1)
        .cmd("SET")
        .arg(format!("{base}:seq"))
        .arg(1)
        .query_async::<()>(&mut conn)
        .await
        .unwrap();

    let error = store.list_since(run_id, None, 1).await.unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Corruption);
    let (source, snapshot_len): (Vec<String>, usize) = redis::pipe()
        .cmd("LRANGE")
        .arg(&base)
        .arg(0)
        .arg(-1)
        .cmd("LLEN")
        .arg(&snapshot)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(source, vec![late_raw]);
    assert_eq!(snapshot_len, LEGACY_EVENTS);
    fixture.cleanup().await;
}
