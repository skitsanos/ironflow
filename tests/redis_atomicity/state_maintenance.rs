use ironflow::engine::types::Context;
use ironflow::storage::{PageSize, RunListQuery, StateStore};

use super::redis_support::RedisTest;

const MAINTENANCE_BATCH_SIZE: usize = 32;
const EXPIRED_RUNS: usize = 80;
const LIVE_RUNS: usize = 4;

async fn catalog_counts(fixture: &RedisTest) -> (usize, usize, usize, usize) {
    let mut conn = fixture.connection().await.unwrap();
    redis::pipe()
        .cmd("SCARD")
        .arg(format!("{}runs:index", fixture.prefix))
        .cmd("HLEN")
        .arg(format!("{}run_catalog:v1:members", fixture.prefix))
        .cmd("ZCARD")
        .arg(format!("{}run_catalog:v1:all", fixture.prefix))
        .cmd("ZCARD")
        .arg(format!("{}run_catalog:v1:status:pending", fixture.prefix))
        .query_async(&mut conn)
        .await
        .unwrap()
}

#[tokio::test]
async fn redis_native_pages_apply_bounded_global_catalog_maintenance() {
    let Some(fixture) = RedisTest::connect("state_catalog_maintenance").await else {
        return;
    };
    let writer = fixture.state_store(None).await;
    let expired_ids = (0..EXPIRED_RUNS)
        .map(|index| format!("expired-{index:03}"))
        .collect::<Vec<_>>();
    for run_id in &expired_ids {
        writer
            .init_run(run_id, "expired-flow", &Context::new())
            .await
            .unwrap();
    }

    // Give every cold hash an already-elapsed Redis expiry while retaining
    // the Set/Hash/Sorted-Set catalog entries that do not support field TTLs.
    let mut conn = fixture.connection().await.unwrap();
    let mut expiry = redis::pipe();
    for run_id in &expired_ids {
        expiry
            .cmd("PEXPIRE")
            .arg(format!("{}runs:{run_id}", fixture.prefix))
            .arg(1_u8)
            .ignore();
    }
    expiry.query_async::<()>(&mut conn).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let live_ids = (0..LIVE_RUNS)
        .map(|index| format!("live-{index:03}"))
        .collect::<Vec<_>>();
    for run_id in &live_ids {
        writer
            .init_run(run_id, "live-flow", &Context::new())
            .await
            .unwrap();
    }
    assert_eq!(
        catalog_counts(&fixture).await,
        (
            EXPIRED_RUNS + LIVE_RUNS,
            EXPIRED_RUNS + LIVE_RUNS,
            EXPIRED_RUNS + LIVE_RUNS,
            EXPIRED_RUNS + LIVE_RUNS
        )
    );

    let query = RunListQuery::new(None, None, PageSize::new(2).unwrap()).unwrap();
    let first_reader = fixture.state_store(None).await;
    let first = first_reader.list_run_summaries_page(&query).await.unwrap();
    assert_eq!(first.items.len(), 2);
    assert!(
        first
            .items
            .iter()
            .all(|summary| summary.id.starts_with("live-"))
    );
    assert_eq!(
        catalog_counts(&fixture).await,
        (
            EXPIRED_RUNS + LIVE_RUNS - MAINTENANCE_BATCH_SIZE,
            EXPIRED_RUNS + LIVE_RUNS - MAINTENANCE_BATCH_SIZE,
            EXPIRED_RUNS + LIVE_RUNS - MAINTENANCE_BATCH_SIZE,
            EXPIRED_RUNS + LIVE_RUNS - MAINTENANCE_BATCH_SIZE,
        ),
        "one user page must inspect exactly one bounded maintenance batch"
    );

    // A second store instance resumes the Redis-persisted cursor rather than
    // starting from the same catalog window.
    let second_reader = fixture.state_store(None).await;
    let second = second_reader.list_run_summaries_page(&query).await.unwrap();
    assert!(
        second
            .items
            .iter()
            .all(|summary| summary.id.starts_with("live-"))
    );
    assert_eq!(
        catalog_counts(&fixture).await.0,
        EXPIRED_RUNS + LIVE_RUNS - 2 * MAINTENANCE_BATCH_SIZE
    );
    let cold_member: bool = redis::cmd("SISMEMBER")
        .arg(format!("{}runs:index", fixture.prefix))
        .arg(expired_ids.last().unwrap())
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(
        cold_member,
        "the deep cold member was not yet maintenance-visible"
    );

    let third = first_reader.list_run_summaries_page(&query).await.unwrap();
    assert!(
        third
            .items
            .iter()
            .all(|summary| summary.id.starts_with("live-"))
    );
    assert_eq!(
        catalog_counts(&fixture).await,
        (LIVE_RUNS, LIVE_RUNS, LIVE_RUNS, LIVE_RUNS)
    );
    let cold_member: bool = redis::cmd("SISMEMBER")
        .arg(format!("{}runs:index", fixture.prefix))
        .arg(expired_ids.last().unwrap())
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(
        !cold_member,
        "maintenance did not reach the deep expired member"
    );

    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_catalog_rebuild_resets_an_invalid_maintenance_cursor() {
    let Some(fixture) = RedisTest::connect("state_maintenance_reset").await else {
        return;
    };
    let store = fixture.state_store(None).await;
    store
        .init_run("maintenance-reset", "flow", &Context::new())
        .await
        .unwrap();
    let query = RunListQuery::new(None, None, PageSize::new(1).unwrap()).unwrap();
    store.list_run_summaries_page(&query).await.unwrap();

    let catalog_key = format!("{}run_catalog:v1:all", fixture.prefix);
    let cursor_key = format!("{}run_catalog:v1:maintenance_cursor", fixture.prefix);
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::pipe()
        .cmd("DEL")
        .arg(&catalog_key)
        .cmd("DEL")
        .arg(&cursor_key)
        .cmd("RPUSH")
        .arg(&cursor_key)
        .arg("wrong-type")
        .query_async(&mut conn)
        .await
        .unwrap();

    let page = store.list_run_summaries_page(&query).await.unwrap();
    assert_eq!(page.items[0].id, "maintenance-reset");
    let cursor_type: String = redis::cmd("TYPE")
        .arg(cursor_key)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        cursor_type, "none",
        "the completed one-entry cycle should leave no stale cursor"
    );
    fixture.cleanup().await;
}
