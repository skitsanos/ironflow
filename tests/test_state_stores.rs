//! Tests for StateStore implementations: JsonStateStore and NullStateStore.

use std::collections::HashMap;

use ironflow::engine::types::*;
use ironflow::storage::StateStore;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::storage::null_store::NullStateStore;

fn test_ctx() -> Context {
    let mut ctx = HashMap::new();
    ctx.insert("key".to_string(), serde_json::json!("value"));
    ctx
}

// ===== NullStateStore =====

#[tokio::test]
async fn null_store_init_and_get() {
    let store = NullStateStore::new();
    store
        .init_run("r1", "test_flow", &test_ctx())
        .await
        .unwrap();

    let info = store.get_run_info("r1").await.unwrap();
    assert_eq!(info.id, "r1");
    assert_eq!(info.flow_name, "test_flow");
    assert_eq!(info.status, RunStatus::Pending);
    assert_eq!(info.ctx.get("key").unwrap(), &serde_json::json!("value"));
}

#[tokio::test]
async fn null_store_set_status() {
    let store = NullStateStore::new();
    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();
    store
        .set_run_status("r1", RunStatus::Running)
        .await
        .unwrap();

    let info = store.get_run_info("r1").await.unwrap();
    assert_eq!(info.status, RunStatus::Running);
}

#[tokio::test]
async fn null_store_upsert_task() {
    let store = NullStateStore::new();
    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();

    let mut task = TaskState::new("step1", "log");
    task.status = TaskStatus::Success;
    store.upsert_task("r1", &task).await.unwrap();

    let info = store.get_run_info("r1").await.unwrap();
    assert!(info.tasks.contains_key("step1"));
    assert_eq!(info.tasks["step1"].status, TaskStatus::Success);
}

#[tokio::test]
async fn null_store_update_ctx() {
    let store = NullStateStore::new();
    store.init_run("r1", "flow", &test_ctx()).await.unwrap();

    let mut update = HashMap::new();
    update.insert("new_key".to_string(), serde_json::json!(42));
    store.update_ctx("r1", &update).await.unwrap();

    let ctx = store.get_ctx("r1").await.unwrap();
    assert_eq!(ctx.get("key").unwrap(), &serde_json::json!("value"));
    assert_eq!(ctx.get("new_key").unwrap(), &serde_json::json!(42));
}

#[tokio::test]
async fn null_store_get_missing_run() {
    let store = NullStateStore::new();
    let result = store.get_run_info("missing").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn null_store_delete_run() {
    let store = NullStateStore::new();
    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();
    store.delete_run("r1").await.unwrap();

    let result = store.get_run_info("r1").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn null_store_list_runs_empty() {
    let store = NullStateStore::new();
    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();
    let runs = store.list_runs(None).await.unwrap();
    assert!(runs.is_empty()); // NullStateStore always returns empty
}

// ===== JsonStateStore =====

#[tokio::test]
async fn json_store_init_and_get() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    store
        .init_run("r1", "test_flow", &test_ctx())
        .await
        .unwrap();
    let info = store.get_run_info("r1").await.unwrap();

    assert_eq!(info.id, "r1");
    assert_eq!(info.flow_name, "test_flow");
    assert_eq!(info.status, RunStatus::Pending);
    assert!(info.started.is_some());
    assert!(info.finished.is_none());
}

#[tokio::test]
async fn json_store_set_status_sets_finished() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();
    store
        .set_run_status("r1", RunStatus::Success)
        .await
        .unwrap();

    let info = store.get_run_info("r1").await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert!(info.finished.is_some());
}

#[tokio::test]
async fn json_store_running_no_finished() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();
    store
        .set_run_status("r1", RunStatus::Running)
        .await
        .unwrap();

    let info = store.get_run_info("r1").await.unwrap();
    assert_eq!(info.status, RunStatus::Running);
    assert!(info.finished.is_none());
}

#[tokio::test]
async fn json_store_upsert_task() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();

    let mut task = TaskState::new("step1", "log");
    task.status = TaskStatus::Running;
    task.attempt = 1;
    store.upsert_task("r1", &task).await.unwrap();

    let info = store.get_run_info("r1").await.unwrap();
    assert_eq!(info.tasks["step1"].status, TaskStatus::Running);
    assert_eq!(info.tasks["step1"].attempt, 1);

    // Update same task
    task.status = TaskStatus::Success;
    store.upsert_task("r1", &task).await.unwrap();

    let info = store.get_run_info("r1").await.unwrap();
    assert_eq!(info.tasks["step1"].status, TaskStatus::Success);
}

#[tokio::test]
async fn json_store_update_ctx_merges() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    let mut initial = HashMap::new();
    initial.insert("a".to_string(), serde_json::json!(1));
    initial.insert("b".to_string(), serde_json::json!(2));
    store.init_run("r1", "flow", &initial).await.unwrap();

    let mut update = HashMap::new();
    update.insert("b".to_string(), serde_json::json!(99));
    update.insert("c".to_string(), serde_json::json!(3));
    store.update_ctx("r1", &update).await.unwrap();

    let ctx = store.get_ctx("r1").await.unwrap();
    assert_eq!(ctx.get("a").unwrap(), &serde_json::json!(1)); // preserved
    assert_eq!(ctx.get("b").unwrap(), &serde_json::json!(99)); // updated
    assert_eq!(ctx.get("c").unwrap(), &serde_json::json!(3)); // new
}

#[tokio::test]
async fn json_store_list_runs() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    store
        .init_run("r1", "flow_a", &HashMap::new())
        .await
        .unwrap();
    store
        .set_run_status("r1", RunStatus::Success)
        .await
        .unwrap();

    store
        .init_run("r2", "flow_b", &HashMap::new())
        .await
        .unwrap();
    store.set_run_status("r2", RunStatus::Failed).await.unwrap();

    let all = store.list_runs(None).await.unwrap();
    assert_eq!(all.len(), 2);

    let success_only = store.list_runs(Some(RunStatus::Success)).await.unwrap();
    assert_eq!(success_only.len(), 1);
    assert_eq!(success_only[0].flow_name, "flow_a");

    let failed_only = store.list_runs(Some(RunStatus::Failed)).await.unwrap();
    assert_eq!(failed_only.len(), 1);
    assert_eq!(failed_only[0].flow_name, "flow_b");
}

#[tokio::test]
async fn json_store_delete_run() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();
    store.delete_run("r1").await.unwrap();

    let result = store.get_run_info("r1").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn json_store_list_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path().join("nonexistent"));

    let runs = store.list_runs(None).await.unwrap();
    assert!(runs.is_empty());
}
