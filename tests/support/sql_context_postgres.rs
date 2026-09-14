use super::*;
use postgres_fixture::PostgresStateTest;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn postgres_concurrent_context_merges() {
    let Some(fixture) = PostgresStateTest::from_env("pg_ctx_merge") else {
        return;
    };
    let first = Arc::new(fixture.state_store().await.unwrap());
    let second = Arc::new(fixture.state_store().await.unwrap());
    concurrent_merges(Arc::clone(&first), Arc::clone(&second), false).await;
    concurrent_merges(Arc::clone(&first), second, true).await;
    merge_semantics(&first).await;
    fixture.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn postgres_context_snapshot_cannot_cross_delete_recreate() {
    let Some(fixture) = PostgresStateTest::from_env("pg_ctx_recreate") else {
        return;
    };
    let first = Arc::new(fixture.state_store().await.unwrap());
    let second = Arc::new(fixture.state_store().await.unwrap());
    delete_recreate_races(first, second).await;
    fixture.cleanup().await.unwrap();
}

async fn execute(pool: &sqlx::AnyPool, sql: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .execute(pool)
        .await
        .unwrap();
}

async fn wait_for_lock(pool: &sqlx::AnyPool, table: &str, count: i64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pg_stat_activity WHERE datname = current_database() \
                 AND pid <> pg_backend_pid() AND state = 'active' AND wait_event_type = 'Lock' \
                 AND strpos(query, $1) > 0",
            )
            .bind(table)
            .fetch_one(pool)
            .await
            .unwrap();
            if waiting >= count {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("context writers must reach the database lock");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn postgres_blocked_unowned_writers_read_after_lock() {
    let Some(fixture) = PostgresStateTest::from_env("pg_ctx_block") else {
        return;
    };
    let first = Arc::new(fixture.state_store().await.unwrap());
    let second = Arc::new(fixture.state_store().await.unwrap());
    let pool = sqlx::AnyPool::connect(fixture.url()).await.unwrap();
    let runs = fixture.table("runs");
    first
        .init_run("blocked", "flow", &Context::new())
        .await
        .unwrap();
    let mut blocker = pool.begin().await.unwrap();
    let sql = format!("SELECT id FROM {runs} WHERE id = 'blocked' FOR UPDATE");
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .execute(&mut *blocker)
        .await
        .unwrap();
    let mut writers = JoinSet::new();
    for (index, store) in [Arc::clone(&first), second].into_iter().enumerate() {
        writers.spawn(async move {
            store
                .update_ctx("blocked", &context(&format!("key-{index}"), json!(index)))
                .await
        });
    }
    wait_for_lock(&pool, &runs, 2).await;
    blocker.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(result) = writers.join_next().await {
            result.unwrap().unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        first.get_ctx("blocked").await.unwrap(),
        Context::from([("key-0".into(), json!(0)), ("key-1".into(), json!(1)),])
    );
    fixture.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn postgres_owned_writer_waits_before_reading_unowned_snapshot() {
    let Some(fixture) = PostgresStateTest::from_env("pg_ctx_owned") else {
        return;
    };
    let first = Arc::new(fixture.state_store().await.unwrap());
    let second = Arc::new(fixture.state_store().await.unwrap());
    let pool = sqlx::AnyPool::connect(fixture.url()).await.unwrap();
    let runs = fixture.table("runs");
    let function = fixture.table("pause_context");
    let lock_key = (uuid::Uuid::new_v4().as_u128() & i64::MAX as u128) as i64;
    first
        .init_run_owned(
            "mixed",
            "flow",
            &Context::new(),
            &RunLease::renewed("owner".into()),
        )
        .await
        .unwrap();
    execute(
        &pool,
        &format!(
            "CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN PERFORM pg_advisory_xact_lock({lock_key}); RETURN NEW; END; $$"
        ),
    )
    .await;
    execute(
        &pool,
        &format!(
            "CREATE TRIGGER pause_context BEFORE UPDATE OF ctx ON {runs} \
         FOR EACH ROW EXECUTE FUNCTION {function}()"
        ),
    )
    .await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(lock_key)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let mut writers = JoinSet::new();
    let unowned = Arc::clone(&first);
    writers.spawn(async move {
        unowned
            .update_ctx("mixed", &context("unowned", json!(1)))
            .await
            .unwrap();
    });
    wait_for_lock(&pool, &runs, 1).await;
    writers.spawn(async move {
        assert!(
            second
                .update_ctx_owned("mixed", &context("owned", json!(2)), "owner")
                .await
                .unwrap()
        );
    });
    wait_for_lock(&pool, &runs, 2).await;
    blocker.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(result) = writers.join_next().await {
            result.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        first.get_ctx("mixed").await.unwrap(),
        Context::from([("unowned".into(), json!(1)), ("owned".into(), json!(2)),])
    );
    fixture.cleanup().await.unwrap();
    execute(&pool, &format!("DROP FUNCTION {function}()")).await;
}

#[tokio::test]
async fn postgres_context_errors_and_cancellation_release_locks() {
    let Some(fixture) = PostgresStateTest::from_env("pg_ctx_errors") else {
        return;
    };
    let store = fixture.state_store().await.unwrap();
    let pool = sqlx::AnyPool::connect(fixture.url()).await.unwrap();
    let runs = fixture.table("runs");
    store
        .init_run("errors", "flow", &Context::new())
        .await
        .unwrap();
    execute(
        &pool,
        &format!("UPDATE {runs} SET ctx = '{{broken' WHERE id = 'errors'"),
    )
    .await;
    assert_eq!(
        store
            .update_ctx("errors", &Context::new())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
    execute(
        &pool,
        &format!("UPDATE {runs} SET ctx = '{{}}' WHERE id = 'errors'"),
    )
    .await;
    execute(
        &pool,
        &format!("ALTER TABLE {runs} ADD CONSTRAINT reject_context CHECK (ctx = '{{}}')"),
    )
    .await;
    assert!(
        store
            .update_ctx("errors", &context("rejected", json!(true)))
            .await
            .is_err()
    );
    assert!(store.get_ctx("errors").await.unwrap().is_empty());
    execute(
        &pool,
        &format!("ALTER TABLE {runs} DROP CONSTRAINT reject_context"),
    )
    .await;
    let mut blocker = pool.begin().await.unwrap();
    let sql = format!("SELECT id FROM {runs} WHERE id = 'errors' FOR UPDATE");
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .execute(&mut *blocker)
        .await
        .unwrap();
    let delta = context("cancelled", json!(true));
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            store.update_ctx("errors", &delta)
        )
        .await
        .is_err()
    );
    blocker.rollback().await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(5),
        store.update_ctx("errors", &context("after", json!(1))),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        store.get_ctx("errors").await.unwrap(),
        context("after", json!(1))
    );
    fixture.cleanup().await.unwrap();
}
