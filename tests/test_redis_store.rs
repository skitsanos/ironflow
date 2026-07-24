#![cfg(feature = "redis")]

#[path = "support/redis.rs"]
mod redis_support;

use std::collections::HashMap;
use std::sync::Arc;

use ironflow::engine::types::*;
use ironflow::storage::redis_store::RedisStateStore;
use ironflow::storage::{PageSize, RunListQuery, StateStore, StorageErrorKind};
use redis_support::RedisTest;

/// Helper: create a RedisStateStore with a unique test prefix.
/// Returns None if Redis is not reachable (tests skip gracefully).
async fn test_store(test_name: &str) -> Option<Arc<RedisStateStore>> {
    let fixture = RedisTest::connect(test_name).await?;
    Some(Arc::new(fixture.state_store(None).await))
}

/// Clean up test keys after a test.
async fn cleanup(store: &RedisStateStore, run_ids: &[&str]) {
    for id in run_ids {
        let _ = store.delete_run(id).await;
    }
}

#[tokio::test]
async fn redis_init_and_get_run() {
    let Some(store) = test_store("init_get").await else {
        return;
    };

    let ctx: Context = HashMap::from([(
        "key".to_string(),
        serde_json::Value::String("value".to_string()),
    )]);

    store.init_run("run-1", "test-flow", &ctx).await.unwrap();

    let info = store.get_run_info("run-1").await.unwrap();
    assert_eq!(info.id, "run-1");
    assert_eq!(info.flow_name, "test-flow");
    assert_eq!(info.status, RunStatus::Pending);
    assert!(info.started.is_some());
    assert!(info.finished.is_none());
    assert_eq!(
        info.ctx.get("key").unwrap(),
        &serde_json::Value::String("value".to_string())
    );

    cleanup(&store, &["run-1"]).await;
}

#[tokio::test]
async fn redis_set_run_status() {
    let Some(store) = test_store("status").await else {
        return;
    };

    store
        .init_run("run-s1", "flow", &Context::new())
        .await
        .unwrap();

    store
        .set_run_status("run-s1", RunStatus::Running)
        .await
        .unwrap();
    let info = store.get_run_info("run-s1").await.unwrap();
    assert_eq!(info.status, RunStatus::Running);
    assert!(info.finished.is_none());

    store
        .set_run_status("run-s1", RunStatus::Success)
        .await
        .unwrap();
    let info = store.get_run_info("run-s1").await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert!(info.finished.is_some());

    cleanup(&store, &["run-s1"]).await;
}

#[tokio::test]
async fn redis_upsert_task() {
    let Some(store) = test_store("task").await else {
        return;
    };

    store
        .init_run("run-t1", "flow", &Context::new())
        .await
        .unwrap();

    let task = TaskState {
        name: "step1".to_string(),
        node_type: "log".to_string(),
        status: TaskStatus::Running,
        attempt: 1,
        started: Some(chrono::Utc::now()),
        finished: None,
        input: None,
        output: None,
        error: None,
    };

    store.upsert_task("run-t1", &task).await.unwrap();

    let info = store.get_run_info("run-t1").await.unwrap();
    assert!(info.tasks.contains_key("step1"));
    assert_eq!(info.tasks["step1"].status, TaskStatus::Running);

    // Update same task
    let task_done = TaskState {
        status: TaskStatus::Success,
        finished: Some(chrono::Utc::now()),
        output: Some(serde_json::json!({"result": "ok"})),
        ..task
    };
    store.upsert_task("run-t1", &task_done).await.unwrap();

    let info = store.get_run_info("run-t1").await.unwrap();
    assert_eq!(info.tasks["step1"].status, TaskStatus::Success);

    cleanup(&store, &["run-t1"]).await;
}

#[tokio::test]
async fn redis_get_and_update_ctx() {
    let Some(store) = test_store("ctx").await else {
        return;
    };

    let initial: Context = HashMap::from([("a".to_string(), serde_json::Value::Number(1.into()))]);

    store.init_run("run-c1", "flow", &initial).await.unwrap();

    let ctx = store.get_ctx("run-c1").await.unwrap();
    assert_eq!(ctx.get("a").unwrap(), &serde_json::Value::Number(1.into()));

    let update: Context = HashMap::from([
        (
            "b".to_string(),
            serde_json::Value::String("hello".to_string()),
        ),
        ("a".to_string(), serde_json::Value::Number(42.into())),
    ]);
    store.update_ctx("run-c1", &update).await.unwrap();

    let ctx = store.get_ctx("run-c1").await.unwrap();
    assert_eq!(ctx.get("a").unwrap(), &serde_json::Value::Number(42.into()));
    assert_eq!(
        ctx.get("b").unwrap(),
        &serde_json::Value::String("hello".to_string())
    );

    cleanup(&store, &["run-c1"]).await;
}

#[tokio::test]
async fn redis_list_runs() {
    let Some(store) = test_store("list").await else {
        return;
    };

    store
        .init_run("run-l1", "flow-a", &Context::new())
        .await
        .unwrap();
    store
        .init_run("run-l2", "flow-b", &Context::new())
        .await
        .unwrap();

    store
        .set_run_status("run-l1", RunStatus::Success)
        .await
        .unwrap();

    // List all
    let all = store.list_runs(None).await.unwrap();
    assert_eq!(all.len(), 2);

    // Filter by status
    let success = store.list_runs(Some(RunStatus::Success)).await.unwrap();
    assert!(success.iter().any(|r| r.id == "run-l1"));
    assert!(!success.iter().any(|r| r.id == "run-l2"));

    let pending = store.list_runs(Some(RunStatus::Pending)).await.unwrap();
    assert!(pending.iter().any(|r| r.id == "run-l2"));

    cleanup(&store, &["run-l1", "run-l2"]).await;
}

#[tokio::test]
async fn redis_summary_listing_is_bounded_and_cursor_driven() {
    let Some(store) = test_store("summary_page").await else {
        return;
    };
    let ids = ["page-0", "page-1", "page-2", "page-3", "page-4"];
    for id in ids {
        store.init_run(id, "flow", &Context::new()).await.unwrap();
    }

    let first_query = RunListQuery::new(None, None, PageSize::new(2).unwrap()).unwrap();
    let first = store.list_run_summaries_page(&first_query).await.unwrap();
    assert_eq!(first.items.len(), 2);
    assert!(first.has_more());

    let second_query = RunListQuery::new(None, first.next, PageSize::new(2).unwrap()).unwrap();
    let second = store.list_run_summaries_page(&second_query).await.unwrap();
    assert_eq!(second.items.len(), 2);
    assert!(second.has_more());

    let third_query = RunListQuery::new(None, second.next, PageSize::new(2).unwrap()).unwrap();
    let third = store.list_run_summaries_page(&third_query).await.unwrap();
    assert_eq!(third.items.len(), 1);
    assert!(!third.has_more());

    cleanup(&store, &ids).await;
}

#[tokio::test]
async fn redis_summary_page_reads_only_its_native_ordered_window() {
    let Some(fixture) = RedisTest::connect("summary_native_window").await else {
        return;
    };
    let store = fixture.state_store(None).await;
    store
        .init_run("000-old-corrupt", "flow", &Context::new())
        .await
        .unwrap();
    for index in 0..120 {
        store
            .init_run(&format!("new-{index:03}"), "flow", &Context::new())
            .await
            .unwrap();
    }

    let mut conn = fixture.connection().await.unwrap();
    let ready: String = redis::cmd("GET")
        .arg(format!("{}run_catalog:v1:ready", fixture.prefix))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(ready, "1");
    let _: usize = redis::cmd("HSET")
        .arg(format!("{}runs:000-old-corrupt", fixture.prefix))
        .arg("summary")
        .arg("not-json")
        .query_async(&mut conn)
        .await
        .unwrap();

    // A catalog scan would deserialize the corrupt oldest record. Native
    // keyset pagination reads only the newest limit + 1 summaries instead.
    let query = RunListQuery::new(None, None, PageSize::new(2).unwrap()).unwrap();
    let page = store.list_run_summaries_page(&query).await.unwrap();
    assert_eq!(page.items.len(), 2);
    assert!(page.has_more());
    assert!(page.items.iter().all(|item| item.id != "000-old-corrupt"));

    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_summary_page_backfills_the_legacy_set_once() {
    let Some(fixture) = RedisTest::connect("summary_legacy_backfill").await else {
        return;
    };
    let mut conn = fixture.connection().await.unwrap();
    for (run_id, started) in [
        (
            "legacy-old",
            chrono::Utc::now() - chrono::Duration::seconds(1),
        ),
        ("legacy-new", chrono::Utc::now()),
    ] {
        let info = RunInfo {
            id: run_id.to_string(),
            flow_name: "legacy-flow".to_string(),
            status: RunStatus::Running,
            started: Some(started),
            finished: None,
            ctx: Context::new(),
            tasks: HashMap::new(),
        };
        let _: () = redis::pipe()
            .cmd("HSET")
            .arg(format!("{}runs:{run_id}", fixture.prefix))
            .arg("info")
            .arg(serde_json::to_string(&info).unwrap())
            .cmd("SADD")
            .arg(format!("{}runs:index", fixture.prefix))
            .arg(run_id)
            .query_async(&mut conn)
            .await
            .unwrap();
    }

    let store = fixture.state_store(None).await;
    let query =
        RunListQuery::new(Some(RunStatus::Running), None, PageSize::new(5).unwrap()).unwrap();
    let page = store.list_run_summaries_page(&query).await.unwrap();
    assert_eq!(
        page.items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["legacy-new", "legacy-old"]
    );
    let (ready, indexed): (String, usize) = redis::pipe()
        .cmd("GET")
        .arg(format!("{}run_catalog:v1:ready", fixture.prefix))
        .cmd("ZCARD")
        .arg(format!("{}run_catalog:v1:all", fixture.prefix))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(ready.len(), 32);
    assert!(ready.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(indexed, 2);
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_summary_status_indexes_follow_mutations_and_deletes() {
    let Some(fixture) = RedisTest::connect("summary_status_indexes").await else {
        return;
    };
    let store = fixture.state_store(None).await;
    store
        .init_run("status-indexed", "flow", &Context::new())
        .await
        .unwrap();

    let pending_query =
        RunListQuery::new(Some(RunStatus::Pending), None, PageSize::new(5).unwrap()).unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&pending_query)
            .await
            .unwrap()
            .items[0]
            .id,
        "status-indexed"
    );

    store
        .set_run_status("status-indexed", RunStatus::Success)
        .await
        .unwrap();
    assert!(
        store
            .list_run_summaries_page(&pending_query)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let success_query =
        RunListQuery::new(Some(RunStatus::Success), None, PageSize::new(5).unwrap()).unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&success_query)
            .await
            .unwrap()
            .items[0]
            .id,
        "status-indexed"
    );

    store.delete_run("status-indexed").await.unwrap();
    assert!(
        store
            .list_run_summaries_page(&success_query)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_summary_page_rebuilds_missing_ordered_indexes() {
    let Some(fixture) = RedisTest::connect("summary_index_rebuild").await else {
        return;
    };
    let store = fixture.state_store(None).await;
    for run_id in ["rebuild-a", "rebuild-b"] {
        store
            .init_run(run_id, "flow", &Context::new())
            .await
            .unwrap();
    }
    let query = RunListQuery::new(None, None, PageSize::new(5).unwrap()).unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&query)
            .await
            .unwrap()
            .items
            .len(),
        2
    );

    let mut conn = fixture.connection().await.unwrap();
    let _: usize = redis::cmd("DEL")
        .arg(format!("{}run_catalog:v1:all", fixture.prefix))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&query)
            .await
            .unwrap()
            .items
            .len(),
        2
    );

    let _: usize = redis::cmd("DEL")
        .arg(format!("{}run_catalog:v1:status:pending", fixture.prefix))
        .query_async(&mut conn)
        .await
        .unwrap();
    let pending_query =
        RunListQuery::new(Some(RunStatus::Pending), None, PageSize::new(5).unwrap()).unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&pending_query)
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_summary_identity_mismatches_are_corruption() {
    let Some(fixture) = RedisTest::connect("summary_identity").await else {
        return;
    };
    let store = fixture.state_store(None).await;
    store
        .init_run("summary-owner", "flow", &Context::new())
        .await
        .unwrap();
    let mut conn = fixture.connection().await.unwrap();
    let mut summary: RunSummary = serde_json::from_str(
        &redis::cmd("HGET")
            .arg(format!("{}runs:summary-owner", fixture.prefix))
            .arg("summary")
            .query_async::<String>(&mut conn)
            .await
            .unwrap(),
    )
    .unwrap();
    summary.id = "different-owner".to_string();
    let _: usize = redis::cmd("HSET")
        .arg(format!("{}runs:summary-owner", fixture.prefix))
        .arg("summary")
        .arg(serde_json::to_string(&summary).unwrap())
        .query_async(&mut conn)
        .await
        .unwrap();

    let query = RunListQuery::new(None, None, PageSize::new(1).unwrap()).unwrap();
    assert_eq!(
        store
            .list_run_summaries_page(&query)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_delete_run() {
    let Some(store) = test_store("delete").await else {
        return;
    };

    store
        .init_run("run-d1", "flow", &Context::new())
        .await
        .unwrap();

    // Verify it exists
    store.get_run_info("run-d1").await.unwrap();

    // Delete it
    store.delete_run("run-d1").await.unwrap();

    // Verify it's gone
    assert_eq!(
        store.get_run_info("run-d1").await.unwrap_err().kind(),
        StorageErrorKind::NotFound
    );

    // Verify it's not in the index
    let runs = store.list_runs(None).await.unwrap();
    assert!(!runs.iter().any(|r| r.id == "run-d1"));
}

#[tokio::test]
async fn redis_run_not_found() {
    let Some(store) = test_store("notfound").await else {
        return;
    };

    assert_eq!(
        store
            .get_run_info("nonexistent-run")
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
    assert_eq!(
        store
            .delete_run("nonexistent-run")
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
}

#[tokio::test]
async fn redis_conflicts_and_missing_mutations_are_typed() {
    let Some(store) = test_store("typed_errors").await else {
        return;
    };
    store
        .init_run("typed-run", "flow", &Context::new())
        .await
        .unwrap();
    assert_eq!(
        store
            .init_run("typed-run", "flow", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );
    assert_eq!(
        store
            .set_run_status("missing-run", RunStatus::Running)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
    cleanup(&store, &["typed-run"]).await;
}

#[tokio::test]
async fn redis_ttl_applied() {
    let Some(fixture) = RedisTest::connect("ttl").await else {
        return;
    };
    let store = fixture.state_store(Some(3600)).await;

    store
        .init_run("run-ttl1", "flow", &Context::new())
        .await
        .unwrap();

    let mut conn = fixture.connection().await.unwrap();
    let ttl: i64 = redis::cmd("TTL")
        .arg(format!("{}runs:run-ttl1", fixture.prefix))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!((1..=3600).contains(&ttl));

    let info = store.get_run_info("run-ttl1").await.unwrap();
    assert_eq!(info.id, "run-ttl1");

    cleanup(&store, &["run-ttl1"]).await;
    fixture.cleanup().await;
}
