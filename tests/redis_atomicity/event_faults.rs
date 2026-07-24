use std::collections::HashMap;

use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::event_store::EventStore;

use super::redis_support::RedisTest;

#[path = "event_faults_extra.rs"]
mod extra;

#[tokio::test]
async fn redis_event_script_preflight_prevents_partial_publish() {
    let Some(fixture) = RedisTest::connect("event_fault").await else {
        return;
    };
    let run_id = "fault-event-run";
    let base = format!("{}events:{run_id}", fixture.prefix);
    let index_key = format!("{base}:index");
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::cmd("SET")
        .arg(&index_key)
        .arg("wrong-type")
        .query_async(&mut conn)
        .await
        .unwrap();
    let event = RunEvent::run(run_id, "flow", RunEventType::RunStarted, RunStatus::Running);
    let store = fixture.event_store(None).await;
    assert!(store.publish(event).await.is_err());

    for key in [&base, &format!("{base}:seq"), &format!("{base}:meta")] {
        let exists: bool = redis::cmd("EXISTS")
            .arg(key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(!exists, "failed publish mutated {key}");
    }
    let value: String = redis::cmd("GET")
        .arg(&index_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(value, "wrong-type");

    let _: () = redis::pipe()
        .cmd("DEL")
        .arg(&index_key)
        .cmd("SET")
        .arg(&base)
        .arg("wrong-type")
        .query_async(&mut conn)
        .await
        .unwrap();
    let event = RunEvent::run(
        run_id,
        "flow",
        RunEventType::RunFinished,
        RunStatus::Success,
    );
    assert!(store.publish(event).await.is_err());
    for key in [
        format!("{base}:index"),
        format!("{base}:seq"),
        format!("{base}:meta"),
    ] {
        let exists: bool = redis::cmd("EXISTS")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(!exists, "wrong-type list failure mutated {key}");
    }

    let seq_key = format!("{base}:seq");
    let _: () = redis::pipe()
        .cmd("DEL")
        .arg(&base)
        .cmd("SET")
        .arg(&seq_key)
        .arg("1.0")
        .query_async(&mut conn)
        .await
        .unwrap();
    let event = RunEvent::run(run_id, "flow", RunEventType::RunFinished, RunStatus::Failed);
    assert!(store.publish(event).await.is_err());
    let seq: String = redis::cmd("GET")
        .arg(&seq_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(seq, "1.0");
    for key in [&base, &format!("{base}:index"), &format!("{base}:meta")] {
        let exists: bool = redis::cmd("EXISTS")
            .arg(key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(!exists, "malformed sequence failure mutated {key}");
    }
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_event_conflicts_legacy_layouts_and_key_aliases_fail_safely() {
    let Some(fixture) = RedisTest::connect("event_compatibility").await else {
        return;
    };
    let store = fixture.event_store(None).await;
    let mut conn = fixture.connection().await.unwrap();

    let conflict_run = "event-conflict";
    let original = RunEvent::run(
        conflict_run,
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    store.publish(original.clone()).await.unwrap();
    let conflict_base = format!("{}events:{conflict_run}", fixture.prefix);
    let before: (String, Vec<String>, HashMap<String, String>, String) = redis::pipe()
        .cmd("GET")
        .arg(format!("{conflict_base}:seq"))
        .cmd("LRANGE")
        .arg(&conflict_base)
        .arg(0)
        .arg(-1)
        .cmd("HGETALL")
        .arg(format!("{conflict_base}:index"))
        .cmd("HGET")
        .arg(format!("{conflict_base}:meta"))
        .arg("layout")
        .query_async(&mut conn)
        .await
        .unwrap();
    let mut conflicting = original;
    conflicting.reason = Some("different payload".to_string());
    assert!(store.publish(conflicting).await.is_err());
    let after: (String, Vec<String>, HashMap<String, String>, String) = redis::pipe()
        .cmd("GET")
        .arg(format!("{conflict_base}:seq"))
        .cmd("LRANGE")
        .arg(&conflict_base)
        .arg(0)
        .arg(-1)
        .cmd("HGETALL")
        .arg(format!("{conflict_base}:index"))
        .cmd("HGET")
        .arg(format!("{conflict_base}:meta"))
        .arg("layout")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(after, before);

    let legacy_run = "legacy-event";
    let legacy = RunEvent::run(
        legacy_run,
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let legacy_base = format!("{}events:{legacy_run}", fixture.prefix);
    let _: () = redis::pipe()
        .cmd("RPUSH")
        .arg(&legacy_base)
        .arg(serde_json::to_string(&legacy).unwrap())
        .cmd("HSET")
        .arg(format!("{legacy_base}:index"))
        .arg(&legacy.id)
        .arg(1)
        .cmd("SET")
        .arg(format!("{legacy_base}:seq"))
        .arg(1)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        store.list_since(legacy_run, None, 10).await.unwrap(),
        vec![legacy]
    );
    let layout: String = redis::cmd("HGET")
        .arg(format!("{legacy_base}:meta"))
        .arg("layout")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(layout, "2");

    let corrupt_run = "corrupt-event";
    let corrupt = RunEvent::run(
        corrupt_run,
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let corrupt_base = format!("{}events:{corrupt_run}", fixture.prefix);
    let _: () = redis::pipe()
        .cmd("RPUSH")
        .arg(&corrupt_base)
        .arg(serde_json::to_string(&corrupt).unwrap())
        .cmd("HSET")
        .arg(format!("{corrupt_base}:index"))
        .arg(&corrupt.id)
        .arg(2)
        .cmd("SET")
        .arg(format!("{corrupt_base}:seq"))
        .arg(1)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(store.list_since(corrupt_run, None, 10).await.is_err());
    let has_marker: bool = redis::cmd("EXISTS")
        .arg(format!("{corrupt_base}:meta"))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!has_marker);

    for run_id in ["alias", "alias:index"] {
        let event = RunEvent::run(run_id, "flow", RunEventType::RunStarted, RunStatus::Running);
        store.publish(event.clone()).await.unwrap();
        assert_eq!(
            store.list_since(run_id, None, 10).await.unwrap(),
            vec![event]
        );
    }
    fixture.cleanup().await;
}
