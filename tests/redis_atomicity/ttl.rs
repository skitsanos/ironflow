use std::collections::HashMap;

use ironflow::engine::types::{Context, RunStatus};
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::StateStore;
use ironflow::storage::event_store::{EventStore, RedisEventStore};
use ironflow::storage::redis_store::RedisStateStore;

use super::redis_support::RedisTest;

// This is the largest seconds value whose millisecond representation remains
// below Redis 8.8 Lua's scientific-notation threshold of 10^14.
const MAX_LUA_SAFE_TTL_SECONDS: u64 = 99_999_999_999;

#[tokio::test]
async fn redis_ttl_boundary_is_safe_for_state_and_event_scripts() {
    let Some(fixture) = RedisTest::connect("ttl_boundary").await else {
        return;
    };
    let run_id = "ttl-boundary-run";
    let state = fixture.state_store(Some(MAX_LUA_SAFE_TTL_SECONDS)).await;
    state
        .init_run(run_id, "flow", &Context::new())
        .await
        .unwrap();
    state
        .update_ctx(
            run_id,
            &HashMap::from([("updated".to_string(), serde_json::json!(true))]),
        )
        .await
        .unwrap();

    let event = RunEvent::run(run_id, "flow", RunEventType::RunStarted, RunStatus::Running);
    let events = fixture.event_store(Some(MAX_LUA_SAFE_TTL_SECONDS)).await;
    events.publish(event.clone()).await.unwrap();

    // Force the compatibility path to copy PTTL through Lua. At the accepted
    // maximum it must still be a plain decimal accepted by PEXPIRE.
    let mut conn = fixture.connection().await.unwrap();
    let base = format!("{}events:{run_id}", fixture.prefix);
    let _: usize = redis::cmd("DEL")
        .arg(format!("{base}:meta"))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        events.list_since(run_id, None, 10).await.unwrap(),
        vec![event]
    );

    for key in [format!("{}runs:{run_id}", fixture.prefix), base] {
        let ttl: i64 = redis::cmd("TTL")
            .arg(key)
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(
            (MAX_LUA_SAFE_TTL_SECONDS as i64 - 60..=MAX_LUA_SAFE_TTL_SECONDS as i64).contains(&ttl)
        );
    }
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_stores_reject_ttls_above_the_script_safe_range() {
    let Some(fixture) = RedisTest::connect("ttl_rejection").await else {
        return;
    };
    let unsafe_ttl = MAX_LUA_SAFE_TTL_SECONDS + 1;

    let state_error = RedisStateStore::new(
        &fixture.url,
        Some(format!("{}state:", fixture.prefix)),
        Some(unsafe_ttl),
    )
    .await
    .err()
    .expect("unsafe state TTL must be rejected");
    assert!(state_error.to_string().contains("must not exceed"));

    let event_error = RedisEventStore::new(
        &fixture.url,
        Some(format!("{}event:", fixture.prefix)),
        Some(unsafe_ttl),
    )
    .await
    .err()
    .expect("unsafe event TTL must be rejected");
    assert!(event_error.to_string().contains("must not exceed"));
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_mutation_scripts_reject_unsafe_ttls_before_their_first_write() {
    let Some(fixture) = RedisTest::connect("ttl_script_preflight").await else {
        return;
    };
    let mut conn = fixture.connection().await.unwrap();
    let unsafe_ttl = MAX_LUA_SAFE_TTL_SECONDS + 1;
    let init_run_key = format!("{}runs:unsafe-init", fixture.prefix);
    let index_key = format!("{}runs:index", fixture.prefix);

    let init_result: redis::RedisResult<i64> = redis::Script::new(include_str!(
        "../../src/storage/redis_store/scripts/init.lua"
    ))
    .key(&init_run_key)
    .key(&index_key)
    .arg("info")
    .arg("summary")
    .arg("revision")
    .arg("incarnation")
    .arg("unsafe-init")
    .arg(unsafe_ttl)
    .invoke_async(&mut conn)
    .await;
    assert!(init_result.is_err());
    let created_keys: usize = redis::cmd("EXISTS")
        .arg(&init_run_key)
        .arg(&index_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(created_keys, 0, "rejected init TTL left Redis state");

    let state = fixture.state_store(None).await;
    state
        .init_run("unsafe-cas", "flow", &Context::new())
        .await
        .unwrap();
    let cas_run_key = format!("{}runs:unsafe-cas", fixture.prefix);
    let before: HashMap<String, String> = redis::cmd("HGETALL")
        .arg(&cas_run_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    let cas_result: redis::RedisResult<i64> = redis::Script::new(include_str!(
        "../../src/storage/redis_store/scripts/cas.lua"
    ))
    .key(&cas_run_key)
    .arg("__ironflow_legacy_revision__")
    .arg(&before["revision"])
    .arg(&before["incarnation"])
    .arg("changed-info")
    .arg("changed-summary")
    .arg(unsafe_ttl)
    .arg("next-revision")
    .invoke_async(&mut conn)
    .await;
    assert!(cas_result.is_err());
    let after: HashMap<String, String> = redis::cmd("HGETALL")
        .arg(&cas_run_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(after, before, "rejected CAS TTL changed the run hash");

    let event = RunEvent::run(
        "unsafe-event-ttl",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let event_base = format!("{}events:unsafe-event-ttl", fixture.prefix);
    let publish_result: redis::RedisResult<i64> = redis::Script::new(include_str!(
        "../../src/storage/event_store/scripts/publish.lua"
    ))
    .key(&event_base)
    .key(format!("{event_base}:index"))
    .key(format!("{event_base}:seq"))
    .key(format!("{event_base}:meta"))
    .key(format!(
        "{}event_deletions:v1:unsafe-event-ttl",
        fixture.prefix
    ))
    .arg(serde_json::to_string(&event).unwrap())
    .arg(&event.id)
    .arg(&event.run_id)
    .arg(unsafe_ttl)
    .invoke_async(&mut conn)
    .await;
    assert!(publish_result.is_err());
    let event_keys: usize = redis::cmd("EXISTS")
        .arg(&event_base)
        .arg(format!("{event_base}:index"))
        .arg(format!("{event_base}:seq"))
        .arg(format!("{event_base}:meta"))
        .arg(format!(
            "{}event_deletions:v1:unsafe-event-ttl",
            fixture.prefix
        ))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(event_keys, 0, "rejected event TTL left Redis state");
    fixture.cleanup().await;
}
