//! Tests for StateStore implementations: JsonStateStore and NullStateStore.

use std::collections::HashMap;

use ironflow::engine::types::*;
use ironflow::storage::StateStore;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::storage::null_store::NullStateStore;
use ironflow::storage::sql_store::SqlStateStore;
use sqlx::Row;

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
    assert!(
        info.finished.is_none(),
        "Running is non-terminal — finished must stay None"
    );
}

#[tokio::test]
async fn null_store_set_terminal_status_records_finished() {
    let store = NullStateStore::new();
    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();
    store
        .set_run_status("r1", RunStatus::Success)
        .await
        .unwrap();

    let info = store.get_run_info("r1").await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert!(
        info.finished.is_some(),
        "terminal status must set finished timestamp"
    );
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

fn sqlite_store_url(dir: &std::path::Path) -> String {
    format!(
        "sqlite://{}?mode=rwc",
        dir.join("state.sqlite").to_string_lossy()
    )
}

#[tokio::test]
async fn sql_store_init_update_and_get() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqlStateStore::new(&sqlite_store_url(dir.path()))
        .await
        .unwrap();

    let mut initial = HashMap::new();
    initial.insert("a".to_string(), serde_json::json!(1));
    store.init_run("r1", "sql_flow", &initial).await.unwrap();
    store
        .set_run_status("r1", RunStatus::Running)
        .await
        .unwrap();

    let mut task = TaskState::new("step1", "log");
    task.status = TaskStatus::Success;
    task.attempt = 2;
    task.output = Some(serde_json::json!({"ok": true}));
    store.upsert_task("r1", &task).await.unwrap();

    let mut ctx_update = HashMap::new();
    ctx_update.insert("b".to_string(), serde_json::json!("two"));
    store.update_ctx("r1", &ctx_update).await.unwrap();

    let info = store.get_run_info("r1").await.unwrap();
    assert_eq!(info.flow_name, "sql_flow");
    assert_eq!(info.status, RunStatus::Running);
    assert_eq!(info.ctx.get("a").unwrap(), &serde_json::json!(1));
    assert_eq!(info.ctx.get("b").unwrap(), &serde_json::json!("two"));
    assert_eq!(info.tasks["step1"].attempt, 2);
    assert_eq!(
        info.tasks["step1"].output.as_ref().unwrap(),
        &serde_json::json!({"ok": true})
    );
}

#[tokio::test]
async fn sql_store_lists_summaries_without_full_context() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqlStateStore::new(&sqlite_store_url(dir.path()))
        .await
        .unwrap();

    let mut ctx = HashMap::new();
    ctx.insert("large".to_string(), serde_json::json!("x".repeat(1024)));
    store.init_run("r1", "sql_flow", &ctx).await.unwrap();
    store
        .set_run_status("r1", RunStatus::Success)
        .await
        .unwrap();
    store
        .init_run("r2", "other_flow", &HashMap::new())
        .await
        .unwrap();
    store.set_run_status("r2", RunStatus::Failed).await.unwrap();

    let summaries = store
        .list_run_summaries(Some(RunStatus::Success))
        .await
        .unwrap();

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "r1");
    assert_eq!(summaries[0].flow_name, "sql_flow");
    assert_eq!(summaries[0].status, RunStatus::Success);
}

#[tokio::test]
async fn sql_store_uses_custom_table_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_store_url(dir.path());
    let store = SqlStateStore::new_with_prefix(&url, Some("tenant_a_"))
        .await
        .unwrap();

    store
        .init_run("r1", "prefixed_flow", &HashMap::new())
        .await
        .unwrap();

    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let row = sqlx::query(
        "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'tenant_a_runs'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>("count"), 1);

    let row = sqlx::query(
        "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'ironflow_runs'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>("count"), 0);
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_sql_store_works_with_custom_table_prefix() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_sql_prefix("pg_state");
    let store = SqlStateStore::new_with_prefix(&url, Some(&prefix))
        .await
        .unwrap();

    store
        .init_run("pg-r1", "pg_flow", &HashMap::new())
        .await
        .unwrap();
    store
        .set_run_status("pg-r1", RunStatus::Success)
        .await
        .unwrap();

    let info = store.get_run_info("pg-r1").await.unwrap();
    assert_eq!(info.flow_name, "pg_flow");
    assert_eq!(info.status, RunStatus::Success);

    drop(store);
    cleanup_postgres_state_tables(&url, &prefix).await;
}

#[cfg(feature = "postgres")]
fn postgres_database_url() -> Option<String> {
    dotenvy::dotenv().ok();
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"))
}

#[cfg(feature = "postgres")]
fn unique_sql_prefix(label: &str) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("{}_{}_", label, &id[..8])
}

#[cfg(feature = "postgres")]
async fn cleanup_postgres_state_tables(url: &str, prefix: &str) {
    if let Ok(pool) = sqlx::AnyPool::connect(url).await {
        let _ = sqlx::query(&format!("DROP TABLE IF EXISTS {}tasks", prefix))
            .execute(&pool)
            .await;
        let _ = sqlx::query(&format!("DROP TABLE IF EXISTS {}runs", prefix))
            .execute(&pool)
            .await;
    }
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

// --- Native list_run_summaries ---

#[tokio::test]
async fn json_store_writes_sidecar_summary() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    store.init_run("r1", "flow", &test_ctx()).await.unwrap();

    let sidecar = dir.path().join("r1.summary.json");
    assert!(
        sidecar.exists(),
        "write_run must create a `<id>.summary.json` sidecar"
    );
    let raw = tokio::fs::read_to_string(&sidecar).await.unwrap();
    let summary: RunSummary = serde_json::from_str(&raw).unwrap();
    assert_eq!(summary.id, "r1");
    assert_eq!(summary.flow_name, "flow");
    assert_eq!(summary.status, RunStatus::Pending);
}

#[tokio::test]
async fn json_store_list_summaries_uses_sidecars_not_full_records() {
    // Proof that `list_run_summaries` reads sidecars: write a run, then
    // corrupt the main record. A full-record listing would fail; the
    // sidecar-based listing must still succeed.
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());
    store.init_run("r1", "flow", &test_ctx()).await.unwrap();

    tokio::fs::write(dir.path().join("r1.json"), "{corrupt garbage}")
        .await
        .unwrap();

    let summaries = store
        .list_run_summaries(None)
        .await
        .expect("summary listing must not load the corrupt main record");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "r1");
}

#[tokio::test]
async fn json_store_delete_removes_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());
    store.init_run("r1", "flow", &test_ctx()).await.unwrap();

    assert!(dir.path().join("r1.summary.json").exists());

    store.delete_run("r1").await.unwrap();

    assert!(!dir.path().join("r1.json").exists());
    assert!(
        !dir.path().join("r1.summary.json").exists(),
        "delete_run must also remove the sidecar"
    );
}

#[tokio::test]
async fn json_store_status_filter_in_summary_listing() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    store.init_run("r1", "a", &test_ctx()).await.unwrap();
    store.init_run("r2", "b", &test_ctx()).await.unwrap();
    store
        .set_run_status("r2", RunStatus::Success)
        .await
        .unwrap();

    let successes = store
        .list_run_summaries(Some(RunStatus::Success))
        .await
        .unwrap();
    assert_eq!(successes.len(), 1);
    assert_eq!(successes[0].id, "r2");

    let pending = store
        .list_run_summaries(Some(RunStatus::Pending))
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "r1");
}
