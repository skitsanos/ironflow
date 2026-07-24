use ironflow::engine::types::Context;
use ironflow::storage::{PageSize, RunListQuery, StateStore};

use super::redis_support::RedisTest;

#[tokio::test]
async fn redis_expired_index_entries_are_swept_conditionally() {
    let Some(fixture) = RedisTest::connect("state_expiry").await else {
        return;
    };
    let store = fixture.state_store(Some(1)).await;
    store
        .init_run("expired-run", "flow", &Context::new())
        .await
        .unwrap();

    for _ in 0..60 {
        if store.get_run_info("expired-run").await.is_err() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(store.get_run_info("expired-run").await.is_err());
    let query = RunListQuery::new(None, None, PageSize::new(5).unwrap()).unwrap();
    assert!(
        store
            .list_run_summaries_page(&query)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(store.list_runs(None).await.unwrap().is_empty());

    let mut conn = fixture.connection().await.unwrap();
    let indexed: bool = redis::cmd("SISMEMBER")
        .arg(format!("{}runs:index", fixture.prefix))
        .arg("expired-run")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!indexed);

    store
        .init_run("live-run", "flow", &Context::new())
        .await
        .unwrap();
    let sweep_result: i64 = redis::Script::new(include_str!(
        "../../src/storage/redis_store/scripts/sweep.lua"
    ))
    .key(format!("{}runs:live-run", fixture.prefix))
    .key(format!("{}runs:index", fixture.prefix))
    .arg("live-run")
    .invoke_async(&mut conn)
    .await
    .unwrap();
    assert_eq!(sweep_result, 0);
    assert!(store.get_run_info("live-run").await.is_ok());
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_catalog_rereads_when_reinitialization_beats_stale_sweep() {
    let Some(fixture) = RedisTest::connect("state_sweep_reinit").await else {
        return;
    };
    let run_id = "reinitialized-run";
    let index_key = format!("{}runs:index", fixture.prefix);
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::cmd("SADD")
        .arg(&index_key)
        .arg(run_id)
        .query_async(&mut conn)
        .await
        .unwrap();

    let store = fixture.state_store(None).await;
    let initializer = fixture.state_store(None).await;
    let mut pause_conn = fixture.connection().await.unwrap();
    let _: () = redis::cmd("CLIENT")
        .arg("PAUSE")
        .arg(200)
        .arg("WRITE")
        .query_async(&mut pause_conn)
        .await
        .unwrap();

    let initialize = tokio::spawn(async move {
        initializer
            .init_run(run_id, "reinitialized-flow", &Context::new())
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let listed = store.list_runs(None).await.unwrap();
    initialize.await.unwrap().unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, run_id);
    assert_eq!(listed[0].flow_name, "reinitialized-flow");
    fixture.cleanup().await;
}
