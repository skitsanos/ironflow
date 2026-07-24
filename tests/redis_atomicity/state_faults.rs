use std::collections::HashMap;

use ironflow::engine::types::{Context, RunStatus, RunSummary};
use ironflow::storage::StateStore;

use super::redis_support::RedisTest;

#[tokio::test]
async fn redis_state_scripts_preflight_faults_and_upgrade_legacy_records() {
    let Some(fixture) = RedisTest::connect("state_faults").await else {
        return;
    };
    let mut conn = fixture.connection().await.unwrap();
    let index_key = format!("{}runs:index", fixture.prefix);
    let run_key = format!("{}runs:fault-run", fixture.prefix);
    let _: () = redis::cmd("SET")
        .arg(&index_key)
        .arg("wrong-type")
        .query_async(&mut conn)
        .await
        .unwrap();
    let store = fixture.state_store(None).await;
    assert!(
        store
            .init_run("fault-run", "flow", &Context::new())
            .await
            .is_err()
    );
    let exists: bool = redis::cmd("EXISTS")
        .arg(&run_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!exists, "failed initialization left an orphan run hash");

    let _: () = redis::cmd("DEL")
        .arg(&index_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    store
        .init_run("delete-run", "flow", &Context::new())
        .await
        .unwrap();
    let delete_key = format!("{}runs:delete-run", fixture.prefix);
    let _: () = redis::pipe()
        .cmd("DEL")
        .arg(&index_key)
        .cmd("SET")
        .arg(&index_key)
        .arg("wrong-type")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(store.delete_run("delete-run").await.is_err());
    let exists: bool = redis::cmd("EXISTS")
        .arg(&delete_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(
        exists,
        "failed deletion removed the run before index validation"
    );

    let _: () = redis::cmd("DEL")
        .arg(&index_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    let legacy = ironflow::engine::types::RunInfo {
        id: "legacy-run".to_string(),
        flow_name: "legacy".to_string(),
        status: RunStatus::Running,
        started: Some(chrono::Utc::now()),
        finished: None,
        ctx: Context::new(),
        tasks: HashMap::new(),
    };
    let legacy_key = format!("{}runs:legacy-run", fixture.prefix);
    let _: () = redis::cmd("HSET")
        .arg(&legacy_key)
        .arg("info")
        .arg(serde_json::to_string(&legacy).unwrap())
        .query_async(&mut conn)
        .await
        .unwrap();
    let update = HashMap::from([("migrated".to_string(), serde_json::json!(true))]);
    store.update_ctx("legacy-run", &update).await.unwrap();
    assert_eq!(store.get_ctx("legacy-run").await.unwrap()["migrated"], true);
    let (revision, incarnation): (Option<String>, Option<String>) = redis::cmd("HMGET")
        .arg(&legacy_key)
        .arg("revision")
        .arg("incarnation")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(revision.is_some());
    assert!(incarnation.is_some());

    store
        .init_run("aba-run", "old-flow", &Context::new())
        .await
        .unwrap();
    let aba_key = format!("{}runs:aba-run", fixture.prefix);
    let (old_info, old_revision, old_incarnation): (String, String, String) = redis::cmd("HMGET")
        .arg(&aba_key)
        .arg("info")
        .arg("revision")
        .arg("incarnation")
        .query_async(&mut conn)
        .await
        .unwrap();
    store.delete_run("aba-run").await.unwrap();
    store
        .init_run("aba-run", "new-flow", &Context::new())
        .await
        .unwrap();
    let new_incarnation: String = redis::cmd("HGET")
        .arg(&aba_key)
        .arg("incarnation")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_ne!(old_incarnation, new_incarnation);

    let mut stale_info: ironflow::engine::types::RunInfo = serde_json::from_str(&old_info).unwrap();
    stale_info
        .ctx
        .insert("stale".to_string(), serde_json::json!(true));
    let stale_summary = RunSummary::from(&stale_info);
    let stale_result: i64 = redis::Script::new(include_str!(
        "../../src/storage/redis_store/scripts/cas.lua"
    ))
    .key(&aba_key)
    .arg("__ironflow_legacy_revision__")
    .arg(&old_revision)
    .arg(&old_incarnation)
    .arg(serde_json::to_string(&stale_info).unwrap())
    .arg(serde_json::to_string(&stale_summary).unwrap())
    .arg(-1)
    .arg("stale-next-revision")
    .invoke_async(&mut conn)
    .await
    .unwrap();
    assert_eq!(stale_result, -1);
    let recreated = store.get_run_info("aba-run").await.unwrap();
    assert_eq!(recreated.flow_name, "new-flow");
    assert!(!recreated.ctx.contains_key("stale"));

    store
        .init_run("index", "reserved-id", &Context::new())
        .await
        .unwrap();
    assert_eq!(
        store.get_run_info("index").await.unwrap().flow_name,
        "reserved-id"
    );
    let index_type: String = redis::cmd("TYPE")
        .arg(&index_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(index_type, "set");
    store.delete_run("index").await.unwrap();
    let recreated_is_indexed: bool = redis::cmd("SISMEMBER")
        .arg(&index_key)
        .arg("aba-run")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(recreated_is_indexed);
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_state_scripts_reject_aliased_keys_before_mutation() {
    let Some(fixture) = RedisTest::connect("state_key_aliases").await else {
        return;
    };
    let mut conn = fixture.connection().await.unwrap();
    let alias_key = format!("{}aliased", fixture.prefix);

    let init_result: redis::RedisResult<i64> = redis::Script::new(include_str!(
        "../../src/storage/redis_store/scripts/init.lua"
    ))
    .key(&alias_key)
    .key(&alias_key)
    .arg("info")
    .arg("summary")
    .arg("revision")
    .arg("incarnation")
    .arg("run-id")
    .arg(-1)
    .invoke_async(&mut conn)
    .await;
    assert!(init_result.is_err());
    let exists: bool = redis::cmd("EXISTS")
        .arg(&alias_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!exists, "aliased initialization created a partial hash");

    let _: usize = redis::cmd("SADD")
        .arg(&alias_key)
        .arg("run-id")
        .query_async(&mut conn)
        .await
        .unwrap();
    for script in [
        include_str!("../../src/storage/redis_store/scripts/delete.lua"),
        include_str!("../../src/storage/redis_store/scripts/sweep.lua"),
    ] {
        let result: redis::RedisResult<i64> = redis::Script::new(script)
            .key(&alias_key)
            .key(&alias_key)
            .arg("run-id")
            .invoke_async(&mut conn)
            .await;
        assert!(result.is_err());
        let member: bool = redis::cmd("SISMEMBER")
            .arg(&alias_key)
            .arg("run-id")
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(member, "aliased state script mutated its shared key");
    }
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_ordered_catalog_scripts_preflight_before_mutation() {
    let Some(fixture) = RedisTest::connect("state_catalog_faults").await else {
        return;
    };
    let mut conn = fixture.connection().await.unwrap();
    let index_key = format!("{}runs:index", fixture.prefix);
    let members_key = format!("{}run_catalog:v1:members", fixture.prefix);
    let all_key = format!("{}run_catalog:v1:all", fixture.prefix);
    let ready_key = format!("{}run_catalog:v1:ready", fixture.prefix);
    let status_keys = [
        "pending",
        "running",
        "success",
        "failed",
        "stalled",
        "cancelled",
    ]
    .map(|status| format!("{}run_catalog:v1:status:{status}", fixture.prefix));

    let init_id = "catalog-init-fault";
    let init_key = format!("{}runs:{init_id}", fixture.prefix);
    let init_info = ironflow::engine::types::RunInfo {
        id: init_id.to_string(),
        flow_name: "flow".to_string(),
        status: RunStatus::Pending,
        started: None,
        finished: None,
        ctx: Context::new(),
        tasks: HashMap::new(),
    };
    let _: () = redis::cmd("SET")
        .arg(&members_key)
        .arg("wrong-type")
        .query_async(&mut conn)
        .await
        .unwrap();
    let init_result: redis::RedisResult<i64> = redis::Script::new(include_str!(
        "../../src/storage/redis_store/scripts/init.lua"
    ))
    .key(&init_key)
    .key(&index_key)
    .key(&members_key)
    .key(&all_key)
    .key(&status_keys[0])
    .key(&status_keys[1])
    .key(&status_keys[2])
    .key(&status_keys[3])
    .key(&status_keys[4])
    .key(&status_keys[5])
    .key(&ready_key)
    .arg(serde_json::to_string(&init_info).unwrap())
    .arg(serde_json::to_string(&RunSummary::from(&init_info)).unwrap())
    .arg("revision")
    .arg("incarnation")
    .arg(init_id)
    .arg(-1)
    .arg(format!("0:{init_id}"))
    .arg("pending")
    .invoke_async(&mut conn)
    .await;
    assert!(init_result.is_err());
    let created: usize = redis::cmd("EXISTS")
        .arg(&init_key)
        .arg(&index_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(created, 0, "catalog preflight left a run or Set index");
    let _: usize = redis::cmd("DEL")
        .arg(&members_key)
        .query_async(&mut conn)
        .await
        .unwrap();

    let store = fixture.state_store(None).await;
    let cas_id = "catalog-cas-fault";
    store
        .init_run(cas_id, "flow", &Context::new())
        .await
        .unwrap();
    let cas_key = format!("{}runs:{cas_id}", fixture.prefix);
    let before: HashMap<String, String> = redis::cmd("HGETALL")
        .arg(&cas_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    let member: String = redis::cmd("HGET")
        .arg(&members_key)
        .arg(cas_id)
        .query_async(&mut conn)
        .await
        .unwrap();
    let mut next_info: ironflow::engine::types::RunInfo =
        serde_json::from_str(&before["info"]).unwrap();
    next_info
        .ctx
        .insert("must-not-commit".to_string(), serde_json::json!(true));
    let _: () = redis::pipe()
        .cmd("DEL")
        .arg(&status_keys[0])
        .cmd("SET")
        .arg(&status_keys[0])
        .arg("wrong-type")
        .query_async(&mut conn)
        .await
        .unwrap();
    let cas_result: redis::RedisResult<i64> = redis::Script::new(include_str!(
        "../../src/storage/redis_store/scripts/cas.lua"
    ))
    .key(&cas_key)
    .key(&members_key)
    .key(&all_key)
    .key(&status_keys[0])
    .key(&status_keys[1])
    .key(&status_keys[2])
    .key(&status_keys[3])
    .key(&status_keys[4])
    .key(&status_keys[5])
    .arg("__ironflow_legacy_revision__")
    .arg(&before["revision"])
    .arg(&before["incarnation"])
    .arg(serde_json::to_string(&next_info).unwrap())
    .arg(serde_json::to_string(&RunSummary::from(&next_info)).unwrap())
    .arg(-1)
    .arg("next-revision")
    .arg(cas_id)
    .arg(&member)
    .arg("pending")
    .invoke_async(&mut conn)
    .await;
    assert!(cas_result.is_err());
    let after: HashMap<String, String> = redis::cmd("HGETALL")
        .arg(&cas_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(after, before, "catalog preflight changed the run hash");

    let upsert_id = "catalog-upsert-fault";
    let upsert_run = format!("{}fault:run", fixture.prefix);
    let upsert_members = format!("{}fault:members", fixture.prefix);
    let upsert_all = format!("{}fault:all", fixture.prefix);
    let upsert_statuses =
        [0, 1, 2, 3, 4, 5].map(|index| format!("{}fault:status:{index}", fixture.prefix));
    let upsert_info = ironflow::engine::types::RunInfo {
        id: upsert_id.to_string(),
        flow_name: "flow".to_string(),
        status: RunStatus::Pending,
        started: None,
        finished: None,
        ctx: Context::new(),
        tasks: HashMap::new(),
    };
    let _: () = redis::pipe()
        .cmd("HSET")
        .arg(&upsert_run)
        .arg("info")
        .arg(serde_json::to_string(&upsert_info).unwrap())
        .arg("revision")
        .arg("current-revision")
        .cmd("SET")
        .arg(&upsert_all)
        .arg("wrong-type")
        .query_async(&mut conn)
        .await
        .unwrap();
    let upsert_result: redis::RedisResult<i64> = redis::Script::new(include_str!(
        "../../src/storage/redis_store/scripts/catalog_upsert.lua"
    ))
    .key(&upsert_run)
    .key(&upsert_members)
    .key(&upsert_all)
    .key(&upsert_statuses[0])
    .key(&upsert_statuses[1])
    .key(&upsert_statuses[2])
    .key(&upsert_statuses[3])
    .key(&upsert_statuses[4])
    .key(&upsert_statuses[5])
    .arg("__ironflow_legacy_revision__")
    .arg("current-revision")
    .arg(serde_json::to_string(&RunSummary::from(&upsert_info)).unwrap())
    .arg(upsert_id)
    .arg(format!("0:{upsert_id}"))
    .arg("pending")
    .invoke_async(&mut conn)
    .await;
    assert!(upsert_result.is_err());
    let (summary_exists, member_exists): (bool, bool) = redis::pipe()
        .cmd("HEXISTS")
        .arg(&upsert_run)
        .arg("summary")
        .cmd("HEXISTS")
        .arg(&upsert_members)
        .arg(upsert_id)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!summary_exists);
    assert!(!member_exists);
    fixture.cleanup().await;
}
