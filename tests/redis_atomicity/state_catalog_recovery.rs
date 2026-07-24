use std::time::Duration;

use ironflow::engine::types::{Context, RunStatus};
use ironflow::storage::{PageSize, RunListQuery, StateStore};

use super::redis_support::RedisTest;

const MAINTENANCE_BATCH_SIZE: usize = 32;

fn page_query(status: Option<RunStatus>, limit: usize) -> RunListQuery {
    RunListQuery::new(status, None, PageSize::new(limit).unwrap()).unwrap()
}

fn catalog_key(prefix: &str, suffix: &str) -> String {
    format!("{prefix}run_catalog:v1:{suffix}")
}

async fn catalog_member(fixture: &RedisTest, run_id: &str) -> String {
    let mut conn = fixture.connection().await.unwrap();
    redis::cmd("HGET")
        .arg(catalog_key(&fixture.prefix, "members"))
        .arg(run_id)
        .query_async(&mut conn)
        .await
        .unwrap()
}

#[tokio::test]
async fn concurrent_catalog_rebuilders_never_serve_a_partial_generation() {
    let Some(fixture) = RedisTest::connect("state_catalog_rebuild_lock").await else {
        return;
    };
    let writer = fixture.state_store(None).await;
    writer
        .init_run("rebuild-older", "flow", &Context::new())
        .await
        .unwrap();
    writer
        .init_run("rebuild-newer", "flow", &Context::new())
        .await
        .unwrap();
    let query = page_query(None, 10);
    assert_eq!(
        writer
            .list_run_summaries_page(&query)
            .await
            .unwrap()
            .items
            .len(),
        2
    );

    let partial_member = catalog_member(&fixture, "rebuild-newer").await;
    let lock_key = catalog_key(&fixture.prefix, "rebuild_lock");
    let ready_key = catalog_key(&fixture.prefix, "ready");
    let members_key = catalog_key(&fixture.prefix, "members");
    let all_key = catalog_key(&fixture.prefix, "all");
    let status_keys = [
        "pending",
        "running",
        "success",
        "failed",
        "stalled",
        "cancelled",
    ]
    .map(|status| catalog_key(&fixture.prefix, &format!("status:{status}")));
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::cmd("SET")
        .arg(&lock_key)
        .arg("external-rebuild-owner")
        .arg("PX")
        .arg(5_000_u16)
        .query_async(&mut conn)
        .await
        .unwrap();
    let _: () = redis::cmd("DEL")
        .arg(&ready_key)
        .arg(&members_key)
        .arg(&all_key)
        .arg(&status_keys)
        .arg(catalog_key(&fixture.prefix, "maintenance_cursor"))
        .arg(catalog_key(&fixture.prefix, "maintenance_high_water"))
        .query_async(&mut conn)
        .await
        .unwrap();
    let _: () = redis::pipe()
        .cmd("HSET")
        .arg(&members_key)
        .arg("rebuild-newer")
        .arg(&partial_member)
        .cmd("ZADD")
        .arg(&all_key)
        .arg(0_u8)
        .arg(&partial_member)
        .cmd("ZADD")
        .arg(&status_keys[0])
        .arg(0_u8)
        .arg(&partial_member)
        .cmd("SET")
        .arg(&ready_key)
        .arg("1")
        .query_async(&mut conn)
        .await
        .unwrap();

    let first_store = fixture.state_store(None).await;
    let first_query = query.clone();
    let first =
        tokio::spawn(async move { first_store.list_run_summaries_page(&first_query).await });
    let second_store = fixture.state_store(None).await;
    let second_query = query.clone();
    let second =
        tokio::spawn(async move { second_store.list_run_summaries_page(&second_query).await });

    tokio::time::sleep(Duration::from_millis(75)).await;
    assert!(!first.is_finished(), "a reader bypassed the rebuild owner");
    assert!(!second.is_finished(), "a reader bypassed the rebuild owner");
    let _: usize = redis::cmd("DEL")
        .arg(&lock_key)
        .query_async(&mut conn)
        .await
        .unwrap();

    for page in [first, second] {
        let page = tokio::time::timeout(Duration::from_secs(5), page)
            .await
            .expect("catalog reader did not resume after rebuild ownership released")
            .unwrap()
            .unwrap();
        assert_eq!(page.items.len(), 2);
        assert!(page.items.iter().any(|run| run.id == "rebuild-older"));
        assert!(page.items.iter().any(|run| run.id == "rebuild-newer"));
    }
    let generation: String = redis::cmd("GET")
        .arg(ready_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(generation.len(), 32);
    fixture.cleanup().await;
}

#[tokio::test]
async fn maintenance_cycle_wraps_at_its_snapshot_during_continuous_inserts() {
    let Some(fixture) = RedisTest::connect("state_maintenance_high_water").await else {
        return;
    };
    let store = fixture.state_store(None).await;
    for index in 0..40 {
        store
            .init_run(&format!("old-{index:03}"), "flow", &Context::new())
            .await
            .unwrap();
    }
    let query = page_query(None, 1);
    store.list_run_summaries_page(&query).await.unwrap();

    let cursor_key = catalog_key(&fixture.prefix, "maintenance_cursor");
    let high_water_key = catalog_key(&fixture.prefix, "maintenance_high_water");
    let mut conn = fixture.connection().await.unwrap();
    let state_exists: usize = redis::cmd("EXISTS")
        .arg(&cursor_key)
        .arg(&high_water_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        state_exists, 2,
        "the first bounded cycle did not remain open"
    );

    let _: usize = redis::cmd("DEL")
        .arg(format!("{}runs:old-000", fixture.prefix))
        .query_async(&mut conn)
        .await
        .unwrap();
    for wave in 1..=2 {
        for index in 0..MAINTENANCE_BATCH_SIZE {
            store
                .init_run(
                    &format!("zz-wave-{wave}-{index:03}"),
                    "flow",
                    &Context::new(),
                )
                .await
                .unwrap();
        }
        store.list_run_summaries_page(&query).await.unwrap();
        if wave == 1 {
            let state_exists: usize = redis::cmd("EXISTS")
                .arg(&cursor_key)
                .arg(&high_water_key)
                .query_async(&mut conn)
                .await
                .unwrap();
            assert_eq!(state_exists, 0, "the original cycle did not close");
        }
    }

    let indexed: bool = redis::cmd("SISMEMBER")
        .arg(format!("{}runs:index", fixture.prefix))
        .arg("old-000")
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(
        !indexed,
        "continuous newer inserts starved an expired member behind the cursor"
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn bounded_maintenance_repairs_balanced_status_index_corruption() {
    let Some(fixture) = RedisTest::connect("state_maintenance_status_repair").await else {
        return;
    };
    let store = fixture.state_store(None).await;
    store
        .init_run("pending-run", "flow", &Context::new())
        .await
        .unwrap();
    store
        .init_run("success-run", "flow", &Context::new())
        .await
        .unwrap();
    store
        .set_run_status("success-run", RunStatus::Success)
        .await
        .unwrap();
    store
        .list_run_summaries_page(&page_query(None, 10))
        .await
        .unwrap();

    let member = catalog_member(&fixture, "pending-run").await;
    let pending_key = catalog_key(&fixture.prefix, "status:pending");
    let success_key = catalog_key(&fixture.prefix, "status:success");
    let ready_key = catalog_key(&fixture.prefix, "ready");
    let mut conn = fixture.connection().await.unwrap();
    let generation_before: String = redis::cmd("GET")
        .arg(&ready_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    let _: () = redis::pipe()
        .cmd("ZREM")
        .arg(&pending_key)
        .arg(&member)
        .cmd("ZADD")
        .arg(&success_key)
        .arg(0_u8)
        .arg(&member)
        .query_async(&mut conn)
        .await
        .unwrap();

    let page = store
        .list_run_summaries_page(&page_query(Some(RunStatus::Pending), 10))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, "pending-run");
    let generation_after: String = redis::cmd("GET")
        .arg(ready_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        generation_after, generation_before,
        "balanced corruption should heal in bounded maintenance, not a rebuild"
    );
    fixture.cleanup().await;
}
