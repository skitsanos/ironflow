//! IF-184: a deleted run's event fence belongs to the deleted incarnation.

use std::sync::Arc;

use super::redis_support::RedisTest;

#[path = "../event_store/fence_contract.rs"]
mod fence_contract;

#[tokio::test]
async fn redis_resubmitted_run_has_complete_events() {
    let Some(fixture) = RedisTest::connect("event_incarnation").await else {
        return;
    };
    fence_contract::assert_resubmitted_run_has_complete_events(
        Arc::new(fixture.event_store(Some(60)).await),
        "redis-resubmit",
    )
    .await;
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_missing_run_delete_leaves_no_fence() {
    let Some(fixture) = RedisTest::connect("event_incarnation").await else {
        return;
    };
    for ttl in [None, Some(60)] {
        let run_id = format!("redis-missing-run-{}", ttl.is_some());
        fence_contract::assert_missing_run_delete_leaves_no_fence(
            &fixture.event_store(ttl).await,
            &run_id,
        )
        .await;
    }
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_fence_is_scoped_to_one_incarnation() {
    let Some(fixture) = RedisTest::connect("event_incarnation").await else {
        return;
    };
    for ttl in [None, Some(60)] {
        let run_id = format!("redis-incarnations-{}", ttl.is_some());
        fence_contract::assert_fence_is_scoped_to_one_incarnation(
            &fixture.event_store(ttl).await,
            &run_id,
        )
        .await;
    }
    fixture.cleanup().await;
}
