use std::collections::HashMap;

use ironflow::engine::types::{Context, RunInfo, RunStatus};
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::event_store::EventStore;
use ironflow::storage::{PageSize, RunListQuery, StateStore};

use super::redis_support::RedisTest;

#[tokio::test]
async fn redis_state_migrates_non_aliasing_legacy_unsafe_keys() {
    let Some(fixture) = RedisTest::connect("state_unsafe_key_migration").await else {
        return;
    };
    let run_id = "legacy:unsafe";
    let legacy_key = format!("{}runs:{run_id}", fixture.prefix);
    let current_key = format!("{}runs:~{}", fixture.prefix, hex::encode(run_id));
    let index_key = format!("{}runs:index", fixture.prefix);
    let legacy = RunInfo {
        id: run_id.to_string(),
        flow_name: "legacy-flow".to_string(),
        status: RunStatus::Running,
        started: Some(chrono::Utc::now()),
        finished: None,
        ctx: Context::new(),
        tasks: HashMap::new(),
    };
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::pipe()
        .cmd("HSET")
        .arg(&legacy_key)
        .arg("info")
        .arg(serde_json::to_string(&legacy).unwrap())
        .cmd("SADD")
        .arg(&index_key)
        .arg(run_id)
        .query_async(&mut conn)
        .await
        .unwrap();

    let store = fixture.state_store(None).await;
    let update = HashMap::from([("migrated".to_string(), serde_json::json!(true))]);
    store.update_ctx(run_id, &update).await.unwrap();
    assert_eq!(store.get_ctx(run_id).await.unwrap()["migrated"], true);

    let (legacy_exists, current_type, indexed): (bool, String, bool) = redis::pipe()
        .cmd("EXISTS")
        .arg(&legacy_key)
        .cmd("TYPE")
        .arg(&current_key)
        .cmd("SISMEMBER")
        .arg(&index_key)
        .arg(run_id)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!legacy_exists);
    assert_eq!(current_type, "hash");
    assert!(indexed);
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_ordered_catalog_does_not_alias_historical_raw_run_keys() {
    let Some(fixture) = RedisTest::connect("state_ordered_catalog_alias").await else {
        return;
    };
    let run_id = "ordered:v1:members";
    let legacy_key = format!("{}runs:{run_id}", fixture.prefix);
    let current_key = format!("{}runs:~{}", fixture.prefix, hex::encode(run_id));
    let catalog_members_key = format!("{}run_catalog:v1:members", fixture.prefix);
    let historical = RunInfo {
        id: run_id.to_string(),
        flow_name: "historical-catalog-name".to_string(),
        status: RunStatus::Running,
        started: Some(chrono::Utc::now()),
        finished: None,
        ctx: Context::new(),
        tasks: HashMap::new(),
    };
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::pipe()
        .cmd("HSET")
        .arg(&legacy_key)
        .arg("info")
        .arg(serde_json::to_string(&historical).unwrap())
        .cmd("SADD")
        .arg(format!("{}runs:index", fixture.prefix))
        .arg(run_id)
        .query_async(&mut conn)
        .await
        .unwrap();

    let store = fixture.state_store(None).await;
    store
        .init_run("ordinary-run", "ordinary", &Context::new())
        .await
        .unwrap();
    let query = RunListQuery::new(None, None, PageSize::new(5).unwrap()).unwrap();
    let page = store.list_run_summaries_page(&query).await.unwrap();
    assert!(page.items.iter().any(|item| item.id == run_id));
    assert!(page.items.iter().any(|item| item.id == "ordinary-run"));

    let (legacy_exists, current_type, catalog_type, raw_info): (bool, String, String, String) =
        redis::pipe()
            .cmd("EXISTS")
            .arg(&legacy_key)
            .cmd("TYPE")
            .arg(&current_key)
            .cmd("TYPE")
            .arg(&catalog_members_key)
            .cmd("HGET")
            .arg(&current_key)
            .arg("info")
            .query_async(&mut conn)
            .await
            .unwrap();
    assert!(!legacy_exists);
    assert_eq!(current_type, "hash");
    assert_eq!(catalog_type, "hash");
    let persisted: RunInfo = serde_json::from_str(&raw_info).unwrap();
    assert_eq!(persisted.flow_name, "historical-catalog-name");
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_events_migrate_non_aliasing_legacy_unsafe_keys() {
    let Some(fixture) = RedisTest::connect("event_unsafe_key_migration").await else {
        return;
    };
    let run_id = "legacy:event";
    let legacy_list = format!("{}events:{run_id}", fixture.prefix);
    let current_list = format!("{}events:~{}", fixture.prefix, hex::encode(run_id));
    let first = RunEvent::run(
        run_id,
        "legacy-flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::pipe()
        .cmd("RPUSH")
        .arg(&legacy_list)
        .arg(serde_json::to_string(&first).unwrap())
        .cmd("HSET")
        .arg(format!("{legacy_list}:index"))
        .arg(&first.id)
        .arg(1)
        .cmd("SET")
        .arg(format!("{legacy_list}:seq"))
        .arg(1)
        .cmd("HSET")
        .arg(format!("{legacy_list}:meta"))
        .arg("layout")
        .arg("2")
        .arg("run_id")
        .arg(run_id)
        .query_async(&mut conn)
        .await
        .unwrap();

    let store = fixture.event_store(None).await;
    assert_eq!(
        store.list_since(run_id, None, 10).await.unwrap(),
        vec![first.clone()]
    );
    let second = RunEvent::run(
        run_id,
        "legacy-flow",
        RunEventType::RunFinished,
        RunStatus::Success,
    );
    store.publish(second.clone()).await.unwrap();
    assert_eq!(
        store.list_since(run_id, None, 10).await.unwrap(),
        vec![first, second]
    );

    for suffix in ["", ":index", ":seq", ":meta"] {
        let legacy_exists: bool = redis::cmd("EXISTS")
            .arg(format!("{legacy_list}{suffix}"))
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(!legacy_exists, "legacy event key was not migrated");
        let current_exists: bool = redis::cmd("EXISTS")
            .arg(format!("{current_list}{suffix}"))
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(current_exists, "encoded event key is missing");
    }
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_encoded_namespace_cannot_be_claimed_as_a_legacy_raw_key() {
    let Some(fixture) = RedisTest::connect("encoded_namespace_collision").await else {
        return;
    };
    // `:` encodes to `~3a`, which is itself a possible historical raw ID.
    let state = fixture.state_store(None).await;
    let events = fixture.event_store(None).await;
    for (run_id, flow_name) in [(":", "encoded-owner"), ("~3a", "raw-lookalike")] {
        state
            .init_run(run_id, flow_name, &Context::new())
            .await
            .unwrap();
        let event = RunEvent::run(
            run_id,
            flow_name,
            RunEventType::RunStarted,
            RunStatus::Running,
        );
        events.publish(event.clone()).await.unwrap();
        assert_eq!(
            events.list_since(run_id, None, 10).await.unwrap(),
            vec![event]
        );
    }

    assert_eq!(
        state.get_run_info(":").await.unwrap().flow_name,
        "encoded-owner"
    );
    assert_eq!(
        state.get_run_info("~3a").await.unwrap().flow_name,
        "raw-lookalike"
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_state_fences_a_historical_raw_encoding_collision() {
    let Some(fixture) = RedisTest::connect("state_historical_encoding_collision").await else {
        return;
    };
    // `:` encodes to `~3a`, which a pre-upgrade run could have used verbatim.
    let historical_id = "~3a";
    let historical_key = format!("{}runs:{historical_id}", fixture.prefix);
    let historical = RunInfo {
        id: historical_id.to_string(),
        flow_name: "historical-flow".to_string(),
        status: RunStatus::Running,
        started: Some(chrono::Utc::now()),
        finished: None,
        ctx: Context::new(),
        tasks: HashMap::new(),
    };
    let assert_historical = |actual: &RunInfo| {
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(&historical).unwrap()
        );
    };
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::pipe()
        .cmd("HSET")
        .arg(&historical_key)
        .arg("info")
        .arg(serde_json::to_string(&historical).unwrap())
        .cmd("SADD")
        .arg(format!("{}runs:index", fixture.prefix))
        .arg(historical_id)
        .query_async(&mut conn)
        .await
        .unwrap();

    let store = fixture.state_store(None).await;
    assert!(store.get_run_info(":").await.is_err());
    assert!(
        store
            .init_run(":", "colliding-flow", &Context::new())
            .await
            .is_err()
    );
    assert!(store.delete_run(":").await.is_err());
    let untouched: String = redis::cmd("HGET")
        .arg(&historical_key)
        .arg("info")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_historical(&serde_json::from_str::<RunInfo>(&untouched).unwrap());

    // Accessing the historical owner migrates it to its own encoded key. The
    // formerly colliding ID can then be initialized and deleted independently.
    assert_historical(&store.get_run_info(historical_id).await.unwrap());
    store
        .init_run(":", "colliding-flow", &Context::new())
        .await
        .unwrap();
    assert_eq!(
        store.get_run_info(":").await.unwrap().flow_name,
        "colliding-flow"
    );
    assert_historical(&store.get_run_info(historical_id).await.unwrap());
    store.delete_run(":").await.unwrap();
    assert_historical(&store.get_run_info(historical_id).await.unwrap());
    fixture.cleanup().await;
}
