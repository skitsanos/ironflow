use std::sync::Arc;

use chrono::{Duration as ChronoDuration, Utc};
use ironflow::engine::types::{Context, RunStatus, TaskState};
use ironflow::storage::sql_store::SqlStateStore;
use ironflow::storage::{StateStore, StorageErrorKind};

fn sqlite_store_url(directory: &std::path::Path) -> String {
    format!(
        "sqlite://{}?mode=rwc",
        directory.join("state.sqlite").display()
    )
}

#[derive(Clone, Copy)]
enum Removal {
    Delete,
    Prune,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_task_upserts_cannot_outlive_delete_or_prune() {
    const RACES_PER_OPERATION: usize = 24;

    let directory = tempfile::tempdir().unwrap();
    let url = sqlite_store_url(directory.path());
    let store = Arc::new(SqlStateStore::new(&url).await.unwrap());
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();

    for removal in [Removal::Delete, Removal::Prune] {
        for index in 0..RACES_PER_OPERATION {
            let run_id = match removal {
                Removal::Delete => format!("sqlite-delete-race-{index:02}"),
                Removal::Prune => format!("sqlite-prune-race-{index:02}"),
            };
            store
                .init_run(&run_id, "flow", &Context::new())
                .await
                .unwrap();
            if matches!(removal, Removal::Prune) {
                store
                    .set_run_status(&run_id, RunStatus::Success)
                    .await
                    .unwrap();
            }

            let barrier = Arc::new(tokio::sync::Barrier::new(3));
            let upsert = {
                let store = Arc::clone(&store);
                let barrier = Arc::clone(&barrier);
                let run_id = run_id.clone();
                tokio::spawn(async move {
                    barrier.wait().await;
                    store
                        .upsert_task(&run_id, &TaskState::new("late-task", "log"))
                        .await
                })
            };
            let remove = {
                let store = Arc::clone(&store);
                let barrier = Arc::clone(&barrier);
                let run_id = run_id.clone();
                tokio::spawn(async move {
                    barrier.wait().await;
                    match removal {
                        Removal::Delete => store.delete_run(&run_id).await.map(|()| 1),
                        Removal::Prune => {
                            store
                                .prune_before(Utc::now() + ChronoDuration::seconds(1))
                                .await
                        }
                    }
                })
            };

            barrier.wait().await;
            let upsert_result = upsert.await.unwrap();
            if let Err(error) = upsert_result {
                assert_eq!(error.kind(), StorageErrorKind::NotFound);
            }
            assert_eq!(remove.await.unwrap().unwrap(), 1);

            let task_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_tasks WHERE run_id = ?")
                    .bind(&run_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let run_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM ironflow_runs WHERE id = ?")
                    .bind(&run_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(task_count, 0, "task survived removal of run '{run_id}'");
            assert_eq!(run_count, 0, "run '{run_id}' survived removal");
        }
    }
}

#[cfg(feature = "postgres")]
mod postgres {
    use std::time::Duration;

    use anyhow::Context as _;
    use sqlx::{AnyConnection, AnyPool, Connection};

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn task_upsert_serializes_with_postgres_delete_and_prune() {
        let Some(url) = postgres_database_url() else {
            eprintln!("Skipping test: DATABASE_URL is not configured for Postgres");
            return;
        };
        let prefix = unique_sql_prefix();
        let runs_table = format!("{prefix}runs");
        let tasks_table = format!("{prefix}tasks");
        let pause_function = format!("{prefix}pause_run_delete");
        let pause_trigger = format!("{prefix}pause_run_delete_trigger");
        let lock_key = (uuid::Uuid::new_v4().as_u128() & i64::MAX as u128) as i64;

        let result: anyhow::Result<()> = async {
            let store = Arc::new(SqlStateStore::new_with_prefix(&url, Some(&prefix)).await?);
            let pool = AnyPool::connect(&url).await?;
            install_delete_pause(
                &pool,
                &runs_table,
                &pause_function,
                &pause_trigger,
                lock_key,
            )
            .await?;

            for removal in [Removal::Delete, Removal::Prune] {
                exercise_postgres_removal_race(
                    Arc::clone(&store),
                    &pool,
                    &url,
                    &runs_table,
                    &tasks_table,
                    lock_key,
                    removal,
                )
                .await?;
            }

            pool.close().await;
            drop(store);
            Ok(())
        }
        .await;

        cleanup_postgres_state_tables(&url, &runs_table, &tasks_table, &pause_function).await;
        result.unwrap();
    }

    async fn exercise_postgres_removal_race(
        store: Arc<SqlStateStore>,
        pool: &AnyPool,
        url: &str,
        runs_table: &str,
        tasks_table: &str,
        lock_key: i64,
        removal: Removal,
    ) -> anyhow::Result<()> {
        let run_id = match removal {
            Removal::Delete => "postgres-delete-task-race",
            Removal::Prune => "postgres-prune-task-race",
        };
        store.init_run(run_id, "flow", &Context::new()).await?;
        store
            .upsert_task(run_id, &TaskState::new("seed-task", "log"))
            .await?;
        if matches!(removal, Removal::Prune) {
            store.set_run_status(run_id, RunStatus::Success).await?;
        }

        let mut controller = AnyConnection::connect(url).await?;
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(lock_key)
            .execute(&mut controller)
            .await?;

        let remove = {
            let store = Arc::clone(&store);
            let run_id = run_id.to_string();
            tokio::spawn(async move {
                match removal {
                    Removal::Delete => store.delete_run(&run_id).await.map(|()| 1),
                    Removal::Prune => {
                        store
                            .prune_before(Utc::now() + ChronoDuration::seconds(1))
                            .await
                    }
                }
            })
        };

        let delete_wait = wait_for_postgres_lock(pool, &format!("DELETE FROM {runs_table}")).await;
        if let Err(error) = delete_wait {
            let _ = unlock_advisory(&mut controller, lock_key).await;
            remove.abort();
            return Err(error);
        }

        let upsert = {
            let store = Arc::clone(&store);
            let run_id = run_id.to_string();
            tokio::spawn(async move {
                store
                    .upsert_task(&run_id, &TaskState::new("late-task", "log"))
                    .await
            })
        };
        let upsert_wait =
            wait_for_postgres_lock(pool, &format!("SELECT id FROM {runs_table}")).await;
        unlock_advisory(&mut controller, lock_key).await?;

        let removed = tokio::time::timeout(Duration::from_secs(5), remove)
            .await
            .context("PostgreSQL run removal did not finish")???;
        let upsert_result = tokio::time::timeout(Duration::from_secs(5), upsert)
            .await
            .context("PostgreSQL task upsert did not finish")??;
        upsert_wait?;
        anyhow::ensure!(removed == 1, "unexpected removal count: {removed}");
        anyhow::ensure!(
            matches!(upsert_result, Err(ref error) if error.kind() == StorageErrorKind::NotFound),
            "late task upsert did not observe the deleted run: {upsert_result:?}"
        );

        let task_count_sql = format!("SELECT COUNT(*) FROM {tasks_table} WHERE run_id = $1");
        let task_count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(task_count_sql.as_str()))
            .bind(run_id)
            .fetch_one(pool)
            .await?;
        let run_count_sql = format!("SELECT COUNT(*) FROM {runs_table} WHERE id = $1");
        let run_count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(run_count_sql.as_str()))
            .bind(run_id)
            .fetch_one(pool)
            .await?;
        anyhow::ensure!(task_count == 0, "task survived removal of run '{run_id}'");
        anyhow::ensure!(run_count == 0, "run '{run_id}' survived removal");
        Ok(())
    }

    async fn wait_for_postgres_lock(pool: &AnyPool, query_fragment: &str) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (\
                 SELECT 1 FROM pg_stat_activity \
                 WHERE datname = current_database() AND pid <> pg_backend_pid() \
                 AND state = 'active' AND wait_event_type = 'Lock' \
                 AND strpos(query, $1) > 0)",
            )
            .bind(query_fragment)
            .fetch_one(pool)
            .await?;
            if waiting {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!("query did not block as expected: {query_fragment}");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn install_delete_pause(
        pool: &AnyPool,
        runs_table: &str,
        pause_function: &str,
        pause_trigger: &str,
        lock_key: i64,
    ) -> anyhow::Result<()> {
        let function_sql = format!(
            "CREATE FUNCTION {pause_function}() RETURNS trigger LANGUAGE plpgsql AS $ironflow$ \
             BEGIN \
                 PERFORM pg_advisory_lock({lock_key}); \
                 PERFORM pg_advisory_unlock({lock_key}); \
                 RETURN OLD; \
             END; \
             $ironflow$"
        );
        sqlx::query(sqlx::AssertSqlSafe(function_sql.as_str()))
            .execute(pool)
            .await?;
        let trigger_sql = format!(
            "CREATE TRIGGER {pause_trigger} BEFORE DELETE ON {runs_table} \
             FOR EACH ROW EXECUTE FUNCTION {pause_function}()"
        );
        sqlx::query(sqlx::AssertSqlSafe(trigger_sql.as_str()))
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn unlock_advisory(connection: &mut AnyConnection, lock_key: i64) -> anyhow::Result<()> {
        let unlocked: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
            .bind(lock_key)
            .fetch_one(connection)
            .await?;
        anyhow::ensure!(unlocked, "PostgreSQL advisory lock was not held");
        Ok(())
    }

    fn postgres_database_url() -> Option<String> {
        dotenvy::dotenv().ok();
        std::env::var("DATABASE_URL")
            .ok()
            .filter(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"))
    }

    fn unique_sql_prefix() -> String {
        let id = uuid::Uuid::new_v4().simple().to_string();
        format!("pg_state_race_{}__", &id[..8])
    }

    async fn cleanup_postgres_state_tables(
        url: &str,
        runs_table: &str,
        tasks_table: &str,
        pause_function: &str,
    ) {
        let Ok(pool) = AnyPool::connect(url).await else {
            return;
        };
        for sql in [
            format!("DROP TABLE IF EXISTS {tasks_table}"),
            format!("DROP TABLE IF EXISTS {runs_table} CASCADE"),
            format!("DROP FUNCTION IF EXISTS {pause_function}()"),
        ] {
            let _ = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .execute(&pool)
                .await;
        }
        pool.close().await;
    }
}
