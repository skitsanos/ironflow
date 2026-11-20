#![cfg(feature = "redis")]
//! Golden tests for the timestamps the Redis stores persist (IF-231 stage 3).
//!
//! Committed against the `chrono` implementation first. Records are written
//! to Redis with the text recorded from that implementation in
//! `storage/fixtures/timestamps/instants.tsv`, the way a replica running it
//! wrote them, and read through the store.

#[path = "support/redis.rs"]
mod redis_support;
#[path = "support/timestamps.rs"]
mod timestamps;

use ironflow::engine::events::{RunEvent, RunEventType};
use ironflow::engine::types::*;
use ironflow::storage::event_store::EventStore;
use ironflow::storage::{PageSize, RunListQuery, StateStore};
use redis_support::RedisTest;
use timestamps::{Row, rows};

fn run_id(row: &Row) -> String {
    format!("row-{}", row.label.replace('_', "-"))
}

#[tokio::test]
async fn redis_state_store_keeps_its_timestamp_forms() {
    let Some(fixture) = RedisTest::connect("timestamp_forms").await else {
        return;
    };
    let mut conn = fixture.connection().await.unwrap();
    let rows = rows();

    // Records as a replica running the old code left them.
    for row in &rows {
        let id = run_id(row);
        let info = format!(
            r#"{{"id":"{id}","flow_name":"f","status":"running","started":"{}","finished":null,"ctx":{{}},"tasks":{{}}}}"#,
            row.serde
        );
        let _: () = redis::pipe()
            .cmd("HSET")
            .arg(format!("{}runs:{id}", fixture.prefix))
            .arg("info")
            .arg(info)
            .cmd("SADD")
            .arg(format!("{}runs:index", fixture.prefix))
            .arg(&id)
            .query_async(&mut conn)
            .await
            .unwrap();
    }

    let store = fixture.state_store(None).await;
    let mut expected: Vec<(i64, String)> =
        rows.iter().map(|row| (row.micros, run_id(row))).collect();
    expected.sort_by(|left, right| right.cmp(left));
    let mut listed = Vec::new();
    let mut after = None;
    loop {
        let query = RunListQuery::new(None, after, PageSize::new(7).unwrap()).unwrap();
        let page = store.list_run_summaries_page(&query).await.unwrap();
        listed.extend(page.items.iter().map(|summary| summary.id.clone()));
        match page.next {
            Some(cursor) => after = Some(cursor),
            None => break,
        }
    }
    assert_eq!(
        listed,
        expected.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
    );

    // A mutation rewrites the record and keeps the recorded bytes.
    for row in &rows {
        let id = run_id(row);
        assert_eq!(
            store.get_run_info(&id).await.unwrap().started,
            Some(row.instant),
            "{id}"
        );
        let mut task = TaskState::new("step", "log");
        task.started = Some(row.instant);
        task.finished = Some(row.instant);
        store.upsert_task(&id, &task).await.unwrap();

        let raw: String = redis::cmd("HGET")
            .arg(format!("{}runs:{id}", fixture.prefix))
            .arg("info")
            .query_async(&mut conn)
            .await
            .unwrap();
        let at = row.serde;
        assert!(
            raw.contains(&format!(r#""started":"{at}","finished":null"#)),
            "{raw}"
        );
        assert!(
            raw.contains(&format!(r#""started":"{at}","finished":"{at}""#)),
            "{raw}"
        );
        let read = store.get_run_info(&id).await.unwrap();
        assert_eq!(read.tasks["step"].finished, Some(row.instant), "{id}");
    }
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_event_store_keeps_its_timestamp_form() {
    let Some(fixture) = RedisTest::connect("timestamp_events").await else {
        return;
    };
    let store = fixture.event_store(None).await;
    let mut expected = Vec::new();
    for row in rows() {
        let mut event = RunEvent::run("golden", "f", RunEventType::RunStarted, RunStatus::Running);
        event.id = row.label.to_string();
        event.timestamp = row.instant;
        store.publish(event.clone()).await.unwrap();
        expected.push(event);
    }
    assert_eq!(
        store.list_since("golden", None, 100).await.unwrap(),
        expected
    );
    fixture.cleanup().await;
}

/// IF-232: a cutoff later in a run's own microsecond keeps the run, as in
/// the SQL and JSON stores.
#[tokio::test]
async fn redis_state_store_prunes_by_whole_microseconds() {
    let Some(fixture) = RedisTest::connect("timestamp_prune").await else {
        return;
    };
    let mut conn = fixture.connection().await.unwrap();
    for (id, started) in [
        ("edge", "2026-01-02T03:04:05.000000500Z"),
        ("older", "2026-01-02T03:04:04Z"),
    ] {
        let info = format!(
            r#"{{"id":"{id}","flow_name":"f","status":"success","started":"{started}","finished":null,"ctx":{{}},"tasks":{{}}}}"#
        );
        let _: () = redis::pipe()
            .cmd("HSET")
            .arg(format!("{}runs:{id}", fixture.prefix))
            .arg("info")
            .arg(info)
            .cmd("SADD")
            .arg(format!("{}runs:index", fixture.prefix))
            .arg(id)
            .query_async(&mut conn)
            .await
            .unwrap();
    }
    let store = fixture.state_store(None).await;

    let inside = timestamps::instant(1_767_323_045, 900);
    assert_eq!(store.prune_before(inside).await.unwrap(), 1);
    assert!(store.get_run_info("older").await.is_err());
    assert!(store.get_run_info("edge").await.is_ok());

    let next_microsecond = timestamps::instant(1_767_323_045, 1_000);
    assert_eq!(store.prune_before(next_microsecond).await.unwrap(), 1);
    assert!(store.get_run_info("edge").await.is_err());
    fixture.cleanup().await;
}
