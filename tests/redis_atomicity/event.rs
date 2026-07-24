use std::collections::HashSet;
use std::sync::Arc;

use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::event_store::EventStore;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

use super::redis_support::RedisTest;
use super::{WRITERS, finish_writers};

#[tokio::test]
async fn redis_concurrent_event_publish_is_ordered_and_idempotent() {
    let Some(fixture) = RedisTest::connect("events_parallel").await else {
        return;
    };
    let run_id = "event-run";
    let barrier = Arc::new(Barrier::new(WRITERS + 1));
    let mut writers = JoinSet::new();
    for _ in 0..WRITERS {
        let store = fixture.event_store(None).await;
        let barrier = barrier.clone();
        writers.spawn(async move {
            let event = RunEvent::run(run_id, "flow", RunEventType::RunStarted, RunStatus::Running);
            barrier.wait().await;
            store.publish(event).await
        });
    }
    let mut pause_conn = fixture.connection().await.unwrap();
    let _: () = redis::cmd("CLIENT")
        .arg("PAUSE")
        .arg(200)
        .arg("WRITE")
        .query_async(&mut pause_conn)
        .await
        .unwrap();
    barrier.wait().await;
    finish_writers(writers).await;

    let store = fixture.event_store(None).await;
    let all = store.list_since(run_id, None, WRITERS + 1).await.unwrap();
    assert_eq!(all.len(), WRITERS);
    assert_eq!(
        all.iter()
            .map(|event| &event.id)
            .collect::<HashSet<_>>()
            .len(),
        WRITERS
    );
    for (index, event) in all.iter().enumerate() {
        let suffix = store
            .list_since(run_id, Some(&event.id), WRITERS + 1)
            .await
            .unwrap();
        assert_eq!(suffix, all[index + 1..]);
    }

    let duplicate = all[0].clone();
    store.publish(duplicate).await.unwrap();
    assert_eq!(
        store.list_since(run_id, None, WRITERS + 1).await.unwrap(),
        all
    );

    let duplicate_run = "duplicate-event-run";
    let duplicate_event = RunEvent::run(
        duplicate_run,
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let duplicate_barrier = Arc::new(Barrier::new(WRITERS + 1));
    let mut duplicate_writers = JoinSet::new();
    for _ in 0..WRITERS {
        let writer = fixture.event_store(None).await;
        let barrier = duplicate_barrier.clone();
        let event = duplicate_event.clone();
        duplicate_writers.spawn(async move {
            barrier.wait().await;
            writer.publish(event).await
        });
    }
    duplicate_barrier.wait().await;
    finish_writers(duplicate_writers).await;
    assert_eq!(
        store
            .list_since(duplicate_run, None, WRITERS + 1)
            .await
            .unwrap(),
        vec![duplicate_event]
    );

    let mut conn = fixture.connection().await.unwrap();
    let base = format!("{}events:{run_id}", fixture.prefix);
    let (seq, list_len, index_len): (i64, usize, usize) = redis::pipe()
        .cmd("GET")
        .arg(format!("{base}:seq"))
        .cmd("LLEN")
        .arg(&base)
        .cmd("HLEN")
        .arg(format!("{base}:index"))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        (seq, list_len, index_len),
        (WRITERS as i64, WRITERS, WRITERS)
    );
    for (index, event) in all.iter().enumerate() {
        let position: usize = redis::cmd("HGET")
            .arg(format!("{base}:index"))
            .arg(&event.id)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert_eq!(position, index + 1);
    }
    fixture.cleanup().await;
}
