//! Tests for StateStore implementations: JsonStateStore and NullStateStore.

use std::collections::HashMap;

use ironflow::engine::types::*;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::storage::null_store::NullStateStore;
use ironflow::storage::sql_store::SqlStateStore;
use ironflow::storage::{PageSize, RunLease, RunListQuery, StateStore, StorageErrorKind};
use sqlx::Row;

#[cfg(feature = "postgres")]
#[path = "support/postgres.rs"]
mod postgres_support;

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
    let error = store.get_run_info("missing").await.unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::NotFound);
}

#[tokio::test]
async fn null_store_delete_run() {
    let store = NullStateStore::new();
    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();
    store.delete_run("r1").await.unwrap();

    let error = store.get_run_info("r1").await.unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::NotFound);
    assert_eq!(
        store.delete_run("r1").await.unwrap_err().kind(),
        StorageErrorKind::NotFound
    );
}

#[tokio::test]
async fn null_store_classifies_duplicate_and_missing_mutations() {
    let store = NullStateStore::new();
    store.init_run("r1", "flow", &Context::new()).await.unwrap();
    assert_eq!(
        store
            .init_run("r1", "flow", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );

    let task = TaskState::new("step", "log");
    for error in [
        store
            .set_run_status("missing", RunStatus::Running)
            .await
            .unwrap_err(),
        store.upsert_task("missing", &task).await.unwrap_err(),
        store
            .update_ctx("missing", &Context::new())
            .await
            .unwrap_err(),
    ] {
        assert_eq!(error.kind(), StorageErrorKind::NotFound);
    }
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

// IF-052: a repeated terminal transition must not move the `finished` timestamp.
#[tokio::test]
async fn json_store_preserves_first_terminal_finished_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());
    store.init_run("r1", "flow", &HashMap::new()).await.unwrap();

    store
        .set_run_status("r1", RunStatus::Success)
        .await
        .unwrap();
    let first = store.get_run_info("r1").await.unwrap().finished.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    store.set_run_status("r1", RunStatus::Failed).await.unwrap();

    let after = store.get_run_info("r1").await.unwrap();
    assert_eq!(after.status, RunStatus::Failed);
    assert_eq!(
        after.finished.unwrap(),
        first,
        "finished must be preserved from the first terminal transition"
    );
}

// IF-051: the JSON store prunes via bounded summary pages and removes only
// terminal runs older than the cutoff.
#[tokio::test]
async fn json_store_prune_before_removes_only_old_terminal_runs() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    for id in ["done-1", "done-2"] {
        store.init_run(id, "flow", &HashMap::new()).await.unwrap();
        store.set_run_status(id, RunStatus::Success).await.unwrap();
    }
    store
        .init_run("live", "flow", &HashMap::new())
        .await
        .unwrap();
    store
        .set_run_status("live", RunStatus::Running)
        .await
        .unwrap();

    // Future cutoff: all runs started before it, but only terminal runs prune.
    let cutoff = chrono::Utc::now() + chrono::Duration::minutes(1);
    assert_eq!(store.prune_before(cutoff).await.unwrap(), 2);
    assert!(store.get_run_info("done-1").await.is_err());
    assert!(store.get_run_info("done-2").await.is_err());
    assert_eq!(
        store.get_run_info("live").await.unwrap().status,
        RunStatus::Running
    );
}

#[tokio::test]
async fn json_store_prune_before_keeps_runs_newer_than_cutoff() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());
    store
        .init_run("recent", "flow", &HashMap::new())
        .await
        .unwrap();
    store
        .set_run_status("recent", RunStatus::Success)
        .await
        .unwrap();

    // Past cutoff: nothing is old enough to prune.
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(5);
    assert_eq!(store.prune_before(cutoff).await.unwrap(), 0);
    assert!(store.get_run_info("recent").await.is_ok());
}

async fn assert_run_lease_contract<F, Fut>(
    owner: &dyn StateStore,
    peer: &dyn StateStore,
    prefix: &str,
    initial_lease: RunLease,
    expire_lease: F,
) where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let expired_id = format!("{prefix}-expired");
    let live_id = format!("{prefix}-live");
    owner
        .init_run_owned(&expired_id, "flow", &Context::new(), &initial_lease)
        .await
        .unwrap();
    // Seed the state reached before the original owner paused beyond its TTL.
    owner
        .set_run_status(&expired_id, RunStatus::Running)
        .await
        .unwrap();
    let mut running_task = TaskState::new("active-task", "log");
    running_task.status = TaskStatus::Running;
    owner.upsert_task(&expired_id, &running_task).await.unwrap();
    expire_lease(expired_id.clone()).await;

    let attempted_renewal = RunLease::at(
        "original-owner",
        chrono::Utc::now() + chrono::Duration::minutes(5),
    );
    assert!(
        !owner
            .renew_run_lease(&expired_id, &attempted_renewal)
            .await
            .unwrap(),
        "an expired owner must not revive its lease"
    );
    for status in [RunStatus::Pending, RunStatus::Success] {
        assert!(
            !owner
                .set_run_status_owned(&expired_id, status, "original-owner")
                .await
                .unwrap(),
            "an expired owner must not mutate run status"
        );
    }

    assert_eq!(
        peer.reconcile_expired_run_leases(chrono::Utc::now())
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        owner.get_run_info(&expired_id).await.unwrap().status,
        RunStatus::Stalled,
    );
    let reconciled = owner.get_run_info(&expired_id).await.unwrap();
    assert_eq!(reconciled.tasks["active-task"].status, TaskStatus::Failed);
    assert!(reconciled.tasks["active-task"].finished.is_some());
    let mut late_task = running_task.clone();
    late_task.status = TaskStatus::Success;
    assert!(
        !owner
            .upsert_task_owned(&expired_id, &late_task, "original-owner")
            .await
            .unwrap(),
        "a reconciled owner must not mutate task state"
    );
    let late_ctx = Context::from([("late".to_string(), serde_json::json!(true))]);
    assert!(
        !owner
            .update_ctx_owned(&expired_id, &late_ctx, "original-owner")
            .await
            .unwrap(),
        "a reconciled owner must not mutate context"
    );
    assert!(
        !owner
            .get_ctx(&expired_id)
            .await
            .unwrap()
            .contains_key("late")
    );

    let abandoned_id = format!("{prefix}-abandoned");
    owner
        .init_run_owned(&abandoned_id, "flow", &Context::new(), &initial_lease)
        .await
        .unwrap();
    owner
        .set_run_status(&abandoned_id, RunStatus::Running)
        .await
        .unwrap();
    expire_lease(abandoned_id.clone()).await;
    peer.delete_run(&abandoned_id).await.unwrap();
    assert_eq!(
        owner.get_run_info(&abandoned_id).await.unwrap_err().kind(),
        StorageErrorKind::NotFound,
        "an expired lease must not prevent deletion of an abandoned run"
    );

    let live = RunLease::at(
        "live-owner",
        chrono::Utc::now() + chrono::Duration::minutes(5),
    );
    owner
        .init_run_owned(&live_id, "flow", &Context::new(), &live)
        .await
        .unwrap();
    assert!(
        owner
            .set_run_status_owned(&live_id, RunStatus::Running, "live-owner")
            .await
            .unwrap()
    );
    assert_eq!(
        peer.reconcile_expired_run_leases(chrono::Utc::now())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        peer.get_run_info(&live_id).await.unwrap().status,
        RunStatus::Running,
        "a new replica must not stall a live peer"
    );
    assert!(
        !peer
            .set_run_status_owned(&live_id, RunStatus::Success, "wrong-owner")
            .await
            .unwrap()
    );
    assert_eq!(
        peer.delete_run(&live_id).await.unwrap_err().kind(),
        StorageErrorKind::Conflict,
        "a peer must not delete a non-terminal run with a live lease"
    );
    assert_eq!(
        owner.get_run_info(&live_id).await.unwrap().status,
        RunStatus::Running,
    );
    assert!(
        owner
            .renew_run_lease(&live_id, &RunLease::renewed("live-owner".to_string()))
            .await
            .unwrap(),
        "a rejected deletion must preserve the live owner's lease"
    );
    assert!(
        owner
            .set_run_status_owned(&live_id, RunStatus::Success, "live-owner")
            .await
            .unwrap()
    );
    assert!(
        !owner
            .renew_run_lease(&live_id, &RunLease::renewed("live-owner".to_string()))
            .await
            .unwrap()
    );
    peer.delete_run(&live_id).await.unwrap();
    assert_eq!(
        owner.get_run_info(&live_id).await.unwrap_err().kind(),
        StorageErrorKind::NotFound,
        "terminal runs must remain deletable"
    );
}

// IF-062: startup reconciliation is fenced by expiring run ownership.
#[tokio::test]
async fn json_run_leases_protect_live_peers_and_reconcile_expired_owners() {
    let dir = tempfile::tempdir().unwrap();
    let owner = JsonStateStore::new(dir.path());
    let peer = JsonStateStore::new(dir.path());
    let expired = RunLease::at(
        "original-owner",
        chrono::Utc::now() - chrono::Duration::seconds(1),
    );
    assert_run_lease_contract(&owner, &peer, "json", expired, |_| async {}).await;
}

#[tokio::test]
async fn sql_run_leases_protect_live_peers_and_reconcile_expired_owners() {
    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_store_url(dir.path());
    let owner = SqlStateStore::new(&url).await.unwrap();
    let peer = SqlStateStore::new(&url).await.unwrap();
    let expiry_url = url.clone();
    assert_run_lease_contract(
        &owner,
        &peer,
        "sql",
        RunLease::at(
            "original-owner",
            chrono::Utc::now() + chrono::Duration::minutes(5),
        ),
        move |run_id| {
            let expiry_url = expiry_url.clone();
            async move {
                let pool = sqlx::AnyPool::connect(&expiry_url).await.unwrap();
                sqlx::query("UPDATE ironflow_run_leases SET expires_micros = 0 WHERE run_id = ?")
                    .bind(run_id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
        },
    )
    .await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_run_leases_protect_live_peers_and_reconcile_expired_owners() {
    let Some(fixture) = postgres_support::PostgresStateTest::from_env("pg_run_lease") else {
        return;
    };
    let owner = fixture.state_store().await.unwrap();
    let peer = fixture.state_store().await.unwrap();
    let expiry_url = fixture.url().to_string();
    let lease_table = fixture.table("run_leases");

    assert_run_lease_contract(
        &owner,
        &peer,
        "postgres",
        RunLease::at(
            "original-owner",
            chrono::Utc::now() + chrono::Duration::minutes(5),
        ),
        move |run_id| {
            let expiry_url = expiry_url.clone();
            let lease_table = lease_table.clone();
            async move {
                let pool = sqlx::AnyPool::connect(&expiry_url).await.unwrap();
                let sql = format!("UPDATE {lease_table} SET expires_micros = 0 WHERE run_id = $1");
                sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(run_id)
                    .execute(&pool)
                    .await
                    .unwrap();
                pool.close().await;
            }
        },
    )
    .await;

    drop(peer);
    drop(owner);
    fixture.cleanup().await.unwrap();
}

#[tokio::test]
async fn sql_run_lease_reconciliation_drains_more_than_one_batch() {
    const RUN_COUNT: usize = 300;

    let directory = tempfile::tempdir().unwrap();
    let url = sqlite_store_url(directory.path());
    let store = SqlStateStore::new(&url).await.unwrap();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    let mut transaction = pool.begin().await.unwrap();
    for index in 0..RUN_COUNT {
        let run_id = format!("expired-batch-{index:03}");
        sqlx::query(
            "INSERT INTO ironflow_runs (id, flow_name, status, ctx) \
             VALUES (?, 'flow', 'running', '{}')",
        )
        .bind(&run_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO ironflow_run_leases (run_id, owner, expires_micros) \
             VALUES (?, 'dead-owner', 0)",
        )
        .bind(run_id)
        .execute(&mut *transaction)
        .await
        .unwrap();
    }
    transaction.commit().await.unwrap();

    assert_eq!(
        store
            .reconcile_expired_run_leases(chrono::Utc::now())
            .await
            .unwrap(),
        RUN_COUNT
    );
    let stalled: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_runs WHERE status = 'stalled'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let leases: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_run_leases")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stalled as usize, RUN_COUNT);
    assert_eq!(leases, 0);
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
async fn sql_store_classifies_missing_conflict_and_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_store_url(dir.path());
    let store = SqlStateStore::new(&url).await.unwrap();

    assert_eq!(
        store.get_run_info("missing").await.unwrap_err().kind(),
        StorageErrorKind::NotFound
    );
    store.init_run("r1", "flow", &Context::new()).await.unwrap();
    assert_eq!(
        store
            .init_run("r1", "flow", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );
    assert_eq!(
        store
            .upsert_task("missing", &TaskState::new("step", "log"))
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
    assert_eq!(
        store.delete_run("missing").await.unwrap_err().kind(),
        StorageErrorKind::NotFound
    );

    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query("UPDATE ironflow_runs SET ctx = '{broken' WHERE id = 'r1'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store.get_ctx("r1").await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
}

#[tokio::test]
async fn sql_delete_and_prune_failures_roll_back_tasks_and_runs() {
    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_store_url(dir.path());
    let store = SqlStateStore::new(&url).await.unwrap();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();

    store
        .init_run("delete-failure", "flow", &Context::new())
        .await
        .unwrap();
    store
        .upsert_task("delete-failure", &TaskState::new("step", "log"))
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_single_delete BEFORE DELETE ON ironflow_runs \
         WHEN OLD.id = 'delete-failure' \
         BEGIN SELECT RAISE(ABORT, 'injected delete failure'); END",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        store.delete_run("delete-failure").await.unwrap_err().kind(),
        StorageErrorKind::Backend
    );
    let retained = store.get_run_info("delete-failure").await.unwrap();
    assert!(retained.tasks.contains_key("step"));
    sqlx::query("DROP TRIGGER reject_single_delete")
        .execute(&pool)
        .await
        .unwrap();

    store
        .init_run("delete-tasks-ignored", "flow", &Context::new())
        .await
        .unwrap();
    store
        .upsert_task("delete-tasks-ignored", &TaskState::new("step", "log"))
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER ignore_single_task_delete BEFORE DELETE ON ironflow_tasks \
         WHEN OLD.run_id = 'delete-tasks-ignored' BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        store
            .delete_run("delete-tasks-ignored")
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    assert!(
        store
            .get_run_info("delete-tasks-ignored")
            .await
            .unwrap()
            .tasks
            .contains_key("step")
    );
    sqlx::query("DROP TRIGGER ignore_single_task_delete")
        .execute(&pool)
        .await
        .unwrap();
    store.delete_run("delete-tasks-ignored").await.unwrap();

    store
        .init_run("delete-recreated-task", "flow", &Context::new())
        .await
        .unwrap();
    store
        .upsert_task("delete-recreated-task", &TaskState::new("step", "log"))
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER recreate_task_after_run_delete AFTER DELETE ON ironflow_runs \
         WHEN OLD.id = 'delete-recreated-task' BEGIN \
         INSERT INTO ironflow_tasks \
         (run_id, name, node_type, status, attempt, input, output, error, started, finished) \
         VALUES (OLD.id, 'ghost', 'log', 'pending', 0, NULL, NULL, NULL, NULL, NULL); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        store
            .delete_run("delete-recreated-task")
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    let retained = store.get_run_info("delete-recreated-task").await.unwrap();
    assert!(retained.tasks.contains_key("step"));
    assert!(!retained.tasks.contains_key("ghost"));
    sqlx::query("DROP TRIGGER recreate_task_after_run_delete")
        .execute(&pool)
        .await
        .unwrap();
    store.delete_run("delete-recreated-task").await.unwrap();

    for id in ["prune-a", "prune-b"] {
        store.init_run(id, "flow", &Context::new()).await.unwrap();
        store
            .upsert_task(id, &TaskState::new("step", "log"))
            .await
            .unwrap();
        store.set_run_status(id, RunStatus::Success).await.unwrap();
    }
    sqlx::query("UPDATE ironflow_runs SET started_micros = 0 WHERE id IN ('prune-a', 'prune-b')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_prune BEFORE DELETE ON ironflow_runs \
         WHEN OLD.id = 'prune-b' \
         BEGIN SELECT RAISE(ABORT, 'injected prune failure'); END",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        store
            .prune_before(chrono::Utc::now())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Backend
    );
    for id in ["prune-a", "prune-b"] {
        let retained = store.get_run_info(id).await.unwrap();
        assert!(retained.tasks.contains_key("step"));
    }

    sqlx::query("DROP TRIGGER reject_prune")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(store.prune_before(chrono::Utc::now()).await.unwrap(), 2);
    for id in ["prune-a", "prune-b"] {
        assert_eq!(
            store.get_run_info(id).await.unwrap_err().kind(),
            StorageErrorKind::NotFound
        );
    }

    store
        .init_run("prune-ignored", "flow", &Context::new())
        .await
        .unwrap();
    store
        .upsert_task("prune-ignored", &TaskState::new("step", "log"))
        .await
        .unwrap();
    store
        .set_run_status("prune-ignored", RunStatus::Success)
        .await
        .unwrap();
    sqlx::query("UPDATE ironflow_runs SET started_micros = 0 WHERE id = 'prune-ignored'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER ignore_prune BEFORE DELETE ON ironflow_runs \
         WHEN OLD.id = 'prune-ignored' \
         BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        store
            .prune_before(chrono::Utc::now())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    let retained = store.get_run_info("prune-ignored").await.unwrap();
    assert!(retained.tasks.contains_key("step"));
    sqlx::query("DROP TRIGGER ignore_prune")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(store.prune_before(chrono::Utc::now()).await.unwrap(), 1);

    store
        .init_run("prune-tasks-ignored", "flow", &Context::new())
        .await
        .unwrap();
    store
        .upsert_task("prune-tasks-ignored", &TaskState::new("step", "log"))
        .await
        .unwrap();
    store
        .set_run_status("prune-tasks-ignored", RunStatus::Success)
        .await
        .unwrap();
    sqlx::query("UPDATE ironflow_runs SET started_micros = 0 WHERE id = 'prune-tasks-ignored'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER ignore_prune_task_delete BEFORE DELETE ON ironflow_tasks \
         WHEN OLD.run_id = 'prune-tasks-ignored' BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        store
            .prune_before(chrono::Utc::now())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    assert!(
        store
            .get_run_info("prune-tasks-ignored")
            .await
            .unwrap()
            .tasks
            .contains_key("step")
    );
    sqlx::query("DROP TRIGGER ignore_prune_task_delete")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(store.prune_before(chrono::Utc::now()).await.unwrap(), 1);
    assert!(store.get_run_info("delete-failure").await.is_ok());
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
async fn postgres_sql_store_reopens_uppercase_prefix_and_uses_listing_index() {
    let Some(url) = postgres_database_url() else {
        eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
        return;
    };
    let prefix = unique_sql_prefix("PG_State");
    let normalized_prefix = prefix.to_ascii_lowercase();
    let result: anyhow::Result<(RunInfo, String)> = async {
        let store = SqlStateStore::new_with_prefix(&url, Some(&prefix)).await?;
        store.init_run("pg-r1", "pg_flow", &HashMap::new()).await?;
        store.set_run_status("pg-r1", RunStatus::Success).await?;
        drop(store);

        // Reopening with the original mixed-case prefix must resolve the same
        // unquoted PostgreSQL identifiers and must not attempt to add the
        // derived timestamp column a second time.
        let reopened = SqlStateStore::new_with_prefix(&url, Some(&prefix)).await?;
        let info = reopened.get_run_info("pg-r1").await?;

        let pool = sqlx::AnyPool::connect(&url).await?;
        let runs_table = format!("{normalized_prefix}runs");
        let tasks_table = format!("{normalized_prefix}tasks");
        let status_index = format!("{normalized_prefix}runs_status_started_id_idx");
        let insert_sql = format!(
            "INSERT INTO {runs_table} \
             (id, flow_name, status, started, started_micros, finished, ctx) \
             SELECT 'plan-' || value, 'plan-flow', \
                    CASE WHEN value % 16 = 0 THEN 'success' ELSE 'failed' END, \
                    CASE WHEN value = 1 THEN NULL ELSE '2026-01-01T00:00:00Z' END, \
                    CASE WHEN value = 1 THEN NULL ELSE value::BIGINT END, NULL, '{{}}' \
             FROM generate_series(1, 8192) AS value"
        );
        sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
            .execute(&pool)
            .await?;
        sqlx::query(sqlx::AssertSqlSafe(format!("ANALYZE {runs_table}")))
            .execute(&pool)
            .await?;

        let mut connection = pool.acquire().await?;
        sqlx::query("SET enable_seqscan = off")
            .execute(&mut *connection)
            .await?;
        let explain_sql = format!(
            "EXPLAIN (COSTS OFF) \
             SELECT r.id, r.flow_name, r.status, r.started, r.finished, \
                    (SELECT COUNT(*) FROM {tasks_table} t WHERE t.run_id = r.id) AS task_count \
             FROM {runs_table} r WHERE r.status = $1 \
             ORDER BY r.started_micros DESC NULLS LAST, r.id DESC LIMIT $2"
        );
        let plan = sqlx::query(sqlx::AssertSqlSafe(explain_sql.as_str()))
            .bind("success")
            .bind(51_i64)
            .fetch_all(&mut *connection)
            .await?
            .iter()
            .map(|row| row.try_get::<String, _>(0))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");
        anyhow::ensure!(plan.contains(&status_index), "{plan}");
        anyhow::ensure!(!plan.contains("Sort"), "{plan}");

        drop(connection);
        pool.close().await;
        drop(reopened);
        Ok((info, plan))
    }
    .await;
    cleanup_postgres_state_tables(&url, &prefix).await;

    let (info, plan) = result.unwrap();
    assert_eq!(info.flow_name, "pg_flow");
    assert_eq!(info.status, RunStatus::Success);
    assert!(plan.contains("Index Scan") || plan.contains("Index Only Scan"));
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
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TABLE IF EXISTS {}tasks",
            prefix
        )))
        .execute(&pool)
        .await;
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TABLE IF EXISTS {}runs",
            prefix
        )))
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

    let error = store.get_run_info("r1").await.unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::NotFound);
    assert_eq!(
        store.delete_run("r1").await.unwrap_err().kind(),
        StorageErrorKind::NotFound
    );
}

#[tokio::test]
async fn json_store_classifies_conflicts_and_corrupt_records() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());
    store.init_run("r1", "flow", &Context::new()).await.unwrap();

    assert_eq!(
        store
            .init_run("r1", "flow", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Conflict
    );

    tokio::fs::write(dir.path().join("r1.json"), "{broken")
        .await
        .unwrap();
    assert_eq!(
        store.get_run_info("r1").await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
    assert_eq!(
        store.list_runs(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
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
async fn json_store_summary_listing_does_not_hide_a_corrupt_primary_header() {
    // A summary is a revision-linked cache, not an independent source of
    // truth. Destroying the primary's revision header must force a full decode
    // and surface the primary corruption.
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());
    store.init_run("r1", "flow", &test_ctx()).await.unwrap();

    tokio::fs::write(dir.path().join("r1.json"), "{corrupt garbage}")
        .await
        .unwrap();

    assert_eq!(
        store.list_run_summaries(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
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

#[tokio::test]
async fn sql_summary_pages_filter_before_limit_and_keep_task_counts() {
    let dir = tempfile::tempdir().unwrap();
    let store_url = sqlite_store_url(dir.path());
    let store = SqlStateStore::new(&store_url).await.unwrap();

    for index in 0..7 {
        let id = format!("page-{index}");
        store.init_run(&id, "flow", &Context::new()).await.unwrap();
        if index % 2 == 0 {
            store.set_run_status(&id, RunStatus::Success).await.unwrap();
            let mut task = TaskState::new("step", "log");
            task.status = TaskStatus::Success;
            store.upsert_task(&id, &task).await.unwrap();
        }
    }

    // Force timestamp ties and a missing timestamp through a second connection
    // so the backend contract, not insertion timing, determines page order.
    let pool = sqlx::AnyPool::connect(&store_url).await.unwrap();
    sqlx::query("UPDATE ironflow_runs SET started = ?, started_micros = ?")
        .bind("2026-01-01T00:00:00.000000000Z")
        .bind(1_767_225_600_000_000_i64)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE ironflow_runs SET started = NULL, started_micros = NULL WHERE id = ?")
        .bind("page-0")
        .execute(&pool)
        .await
        .unwrap();
    let plan = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT id FROM ironflow_runs WHERE status = ? \
         ORDER BY started_micros DESC NULLS LAST, id DESC LIMIT ?",
    )
    .bind("success")
    .bind(3_i64)
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|row| row.try_get::<String, _>("detail").unwrap())
    .collect::<Vec<_>>()
    .join("\n");
    assert!(
        plan.contains("ironflow_runs_status_started_id_idx"),
        "{plan}"
    );
    assert!(!plan.contains("USE TEMP B-TREE"), "{plan}");
    pool.close().await;

    let first_query =
        RunListQuery::new(Some(RunStatus::Success), None, PageSize::new(2).unwrap()).unwrap();
    let first = store.list_run_summaries_page(&first_query).await.unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.items[0].id, "page-6");
    assert_eq!(first.items[1].id, "page-4");
    assert!(
        first
            .items
            .iter()
            .all(|summary| { summary.status == RunStatus::Success && summary.task_count == 1 })
    );

    let second_query = RunListQuery::new(
        Some(RunStatus::Success),
        first.next,
        PageSize::new(2).unwrap(),
    )
    .unwrap();
    let second = store.list_run_summaries_page(&second_query).await.unwrap();
    assert_eq!(second.items.len(), 2);
    assert_eq!(second.items[0].id, "page-2");
    assert_eq!(second.items[1].id, "page-0");
    assert!(!second.has_more());

    let mut ids = first
        .items
        .into_iter()
        .chain(second.items)
        .map(|summary| summary.id)
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(ids, ["page-0", "page-2", "page-4", "page-6"]);
}

#[tokio::test]
async fn sql_store_backfills_legacy_listing_timestamps() {
    sqlx::any::install_default_drivers();
    let dir = tempfile::tempdir().unwrap();
    let store_url = sqlite_store_url(dir.path());
    let pool = sqlx::AnyPool::connect(&store_url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_runs (\
         id TEXT PRIMARY KEY, flow_name TEXT NOT NULL, status TEXT NOT NULL, \
         started TEXT, finished TEXT, ctx TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ironflow_runs (id, flow_name, status, started, finished, ctx) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("legacy-run")
    .bind("legacy-flow")
    .bind("success")
    .bind("2026-01-01T00:00:00Z")
    .bind(Option::<String>::None)
    .bind("{}")
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let store = SqlStateStore::new(&store_url).await.unwrap();

    // Simulate an older process writing after this process completed startup.
    // Page reads must repair the missing derived key before applying order or
    // a cursor, otherwise this newer run would incorrectly sit with NULLs.
    let pool = sqlx::AnyPool::connect(&store_url).await.unwrap();
    sqlx::query(
        "INSERT INTO ironflow_runs \
         (id, flow_name, status, started, finished, ctx) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("late-legacy-run")
    .bind("legacy-flow")
    .bind("success")
    .bind("2030-01-01T00:00:00Z")
    .bind(Option::<String>::None)
    .bind("{}")
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let query = RunListQuery::new(None, None, PageSize::new(1).unwrap()).unwrap();
    let page = store.list_run_summaries_page(&query).await.unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, "late-legacy-run");

    let pool = sqlx::AnyPool::connect(&store_url).await.unwrap();
    let row = sqlx::query("SELECT started_micros FROM ironflow_runs WHERE id = 'late-legacy-run'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        row.try_get::<Option<i64>, _>("started_micros")
            .unwrap()
            .is_some()
    );
}
