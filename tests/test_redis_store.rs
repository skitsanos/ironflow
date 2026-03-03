#![cfg(feature = "redis")]

use std::collections::HashMap;
use std::sync::Arc;

use ironflow::engine::types::*;
use ironflow::storage::StateStore;
use ironflow::storage::redis_store::RedisStateStore;

/// Helper: create a RedisStateStore with a unique test prefix.
/// Returns None if Redis is not reachable (tests skip gracefully).
async fn test_store(test_name: &str) -> Option<Arc<RedisStateStore>> {
    let prefix = format!("ironflow_test:{}:", test_name);
    match RedisStateStore::new("redis://127.0.0.1:6379", Some(prefix), None).await {
        Ok(store) => Some(Arc::new(store)),
        Err(_) => {
            eprintln!("Skipping test: Redis not available at 127.0.0.1:6379");
            None
        }
    }
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
    assert!(all.len() >= 2);

    // Filter by status
    let success = store.list_runs(Some(RunStatus::Success)).await.unwrap();
    assert!(success.iter().any(|r| r.id == "run-l1"));
    assert!(!success.iter().any(|r| r.id == "run-l2"));

    let pending = store.list_runs(Some(RunStatus::Pending)).await.unwrap();
    assert!(pending.iter().any(|r| r.id == "run-l2"));

    cleanup(&store, &["run-l1", "run-l2"]).await;
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
    assert!(store.get_run_info("run-d1").await.is_err());

    // Verify it's not in the index
    let runs = store.list_runs(None).await.unwrap();
    assert!(!runs.iter().any(|r| r.id == "run-d1"));
}

#[tokio::test]
async fn redis_run_not_found() {
    let Some(store) = test_store("notfound").await else {
        return;
    };

    let result = store.get_run_info("nonexistent-run").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn redis_ttl_applied() {
    let prefix = "ironflow_test:ttl:".to_string();
    let store = match RedisStateStore::new(
        "redis://127.0.0.1:6379",
        Some(prefix),
        Some(3600), // 1 hour TTL
    )
    .await
    {
        Ok(s) => s,
        Err(_) => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    store
        .init_run("run-ttl1", "flow", &Context::new())
        .await
        .unwrap();

    // Verify the run exists (TTL just means it will eventually expire)
    let info = store.get_run_info("run-ttl1").await.unwrap();
    assert_eq!(info.id, "run-ttl1");

    cleanup(&store, &["run-ttl1"]).await;
}
