use ironflow::storage::null_store::NullStateStore;
use ironflow::storage::{StateStore, StorageErrorKind};
use sha2::{Digest as _, Sha256};

const TTL: u64 = 7 * 24 * 3600;

// Nested one level below the crate root (inside `mod redis_claims`) so it
// must be declared here rather than there: a `#[path]` on a submodule of an
// inline `mod` resolves relative to a directory named after that inline
// module, not the file it lives in.
#[cfg(feature = "redis")]
#[path = "support/redis.rs"]
mod redis_support;

#[cfg(feature = "postgres")]
#[path = "support/postgres.rs"]
mod postgres_support;

#[tokio::test]
async fn the_null_store_always_grants_the_claim() {
    // It persists nothing and is single-process by definition, so there is no
    // peer to coordinate with.
    let store = NullStateStore::new();
    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn a_store_without_claim_support_refuses_rather_than_duplicating() {
    struct Unsupported;

    #[async_trait::async_trait]
    impl StateStore for Unsupported {
        async fn init_run(
            &self,
            _: &str,
            _: &str,
            _: &ironflow::engine::types::Context,
        ) -> ironflow::storage::StorageResult<()> {
            unimplemented!()
        }
        async fn set_run_status(
            &self,
            _: &str,
            _: ironflow::engine::types::RunStatus,
        ) -> ironflow::storage::StorageResult<()> {
            unimplemented!()
        }
        async fn upsert_task(
            &self,
            _: &str,
            _: &ironflow::engine::types::TaskState,
        ) -> ironflow::storage::StorageResult<()> {
            unimplemented!()
        }
        async fn get_ctx(
            &self,
            _: &str,
        ) -> ironflow::storage::StorageResult<ironflow::engine::types::Context> {
            unimplemented!()
        }
        async fn update_ctx(
            &self,
            _: &str,
            _: &ironflow::engine::types::Context,
        ) -> ironflow::storage::StorageResult<()> {
            unimplemented!()
        }
        async fn get_run_info(
            &self,
            _: &str,
        ) -> ironflow::storage::StorageResult<ironflow::engine::types::RunInfo> {
            unimplemented!()
        }
        async fn list_runs(
            &self,
            _: Option<ironflow::engine::types::RunStatus>,
        ) -> ironflow::storage::StorageResult<Vec<ironflow::engine::types::RunInfo>> {
            unimplemented!()
        }
        async fn list_run_summaries_page(
            &self,
            _: &ironflow::storage::RunListQuery,
        ) -> ironflow::storage::StorageResult<ironflow::storage::RunSummaryPage> {
            unimplemented!()
        }
        async fn delete_run(&self, _: &str) -> ironflow::storage::StorageResult<()> {
            unimplemented!()
        }
    }

    // Failing closed matters: a default of `true` would let every replica fire
    // the same instant, which is the duplicate this method prevents.
    let error = Unsupported
        .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Backend);
    assert!(error.to_string().contains("scheduling"), "{error}");
}

use ironflow::storage::json_store::JsonStateStore;

#[tokio::test]
async fn json_store_grants_a_claim_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
    assert!(
        !store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn longest_public_schedule_name_has_a_portable_json_claim_filename() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());
    let name = "n".repeat(ironflow::scheduler::config::MAX_SCHEDULE_NAME_BYTES);

    assert!(
        store
            .claim_schedule(&name, "America/Argentina/Cordoba@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
    assert!(
        !store
            .claim_schedule(&name, "America/Argentina/Cordoba@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn json_claims_are_scoped_by_name_and_key() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    assert!(
        store
            .claim_schedule("a", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
    assert!(
        store
            .claim_schedule("b", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
    assert!(
        store
            .claim_schedule("a", "UTC@2026-05-02T02:00", TTL)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn two_json_stores_sharing_a_directory_race_and_exactly_one_wins() {
    let dir = tempfile::tempdir().unwrap();
    seed_expired_json_claims(dir.path(), "nightly", 300);
    let first = std::sync::Arc::new(JsonStateStore::new(dir.path()));
    let second = std::sync::Arc::new(JsonStateStore::new(dir.path()));

    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for store in [first, second] {
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            store
                .claim_schedule("nightly", "UTC@2026-05-01T02:00", 1)
                .await
                .unwrap()
        }));
    }

    let mut won = 0;
    for handle in handles {
        if handle.await.unwrap() {
            won += 1;
        }
    }
    assert_eq!(won, 1, "exactly one process must own an instant");
}

#[tokio::test]
async fn json_claim_files_do_not_appear_as_runs() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());
    store
        .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
        .await
        .unwrap();

    // The store scans its directory for `*.json` run records; a claim that
    // matched would corrupt every listing.
    assert!(store.list_runs(None).await.unwrap().is_empty());
    assert!(
        store
            .list_run_summaries_page(
                &ironflow::storage::RunListQuery::new(
                    None,
                    None,
                    ironflow::storage::PageSize::new(16).unwrap()
                )
                .unwrap()
            )
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn expired_json_claims_are_reaped_so_the_directory_cannot_grow_without_bound() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    // A zero TTL makes every existing claim immediately expired.
    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", 0)
            .await
            .unwrap()
    );
    let claim_files = || {
        std::fs::read_dir(dir.path().join(".ironflow-schedule-claims-v1"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".ironflow-schedule-claim")
            })
            .count()
    };
    assert_eq!(claim_files(), 1);

    // The next claim prunes it, so the same key is grantable again.
    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-02T02:00", 0)
            .await
            .unwrap()
    );
    assert_eq!(
        claim_files(),
        1,
        "the expired claim should have been reaped"
    );
}

#[tokio::test]
async fn reaping_one_json_schedule_does_not_delete_another_schedules_claim() {
    // Each schedule derives its own TTL from its own grace window. A
    // short-TTL schedule must not reap a long-TTL schedule's still-valid
    // claim, or the long one could re-fire an instant it already ran.
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    assert!(
        store
            .claim_schedule("long_grace", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
    // A zero-TTL call from a different schedule: it must reap only its own.
    assert!(
        store
            .claim_schedule("short_grace", "UTC@2026-05-01T02:00", 0)
            .await
            .unwrap()
    );
    assert!(
        store
            .claim_schedule("short_grace", "UTC@2026-05-02T02:00", 0)
            .await
            .unwrap()
    );

    // The long-grace schedule's claim survived, so its instant cannot re-fire.
    assert!(
        !store
            .claim_schedule("long_grace", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap(),
        "another schedule's reap deleted this claim"
    );
}

#[tokio::test]
async fn json_cleanup_is_schedule_sharded_cadenced_and_bounded_at_ten_thousand_claims() {
    const EXISTING: usize = 10_001;
    const CLEANUP_BATCH: usize = 256;

    let dir = tempfile::tempdir().unwrap();
    let name = "high_frequency";
    seed_expired_json_claims(dir.path(), name, EXISTING);
    let store = JsonStateStore::new(dir.path());

    assert!(store.claim_schedule(name, "fresh-1", 1).await.unwrap());
    assert!(store.claim_schedule(name, "fresh-2", 1).await.unwrap());

    let prefix = json_claim_prefix(name);
    let remaining = std::fs::read_dir(dir.path().join(".ironflow-schedule-claims-v1"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .count();
    assert_eq!(
        remaining,
        EXISTING - CLEANUP_BATCH + 2,
        "one bounded batch should run; the immediate second claim must not sweep again"
    );

    let schedule_index = dir
        .path()
        .join(".ironflow-schedule-claim-index-v1")
        .join(hex::encode(Sha256::digest(name.as_bytes())));
    assert!(
        schedule_index.is_dir(),
        "claims should use a schedule shard"
    );
}

#[tokio::test]
async fn json_cleanup_migrates_legacy_flat_claims_in_bounded_batches() {
    let dir = tempfile::tempdir().unwrap();
    let name = "legacy_schedule";
    seed_legacy_json_claims(dir.path(), name, 300);
    let store = JsonStateStore::new(dir.path());

    assert!(store.claim_schedule(name, "fresh-1", 0).await.unwrap());
    assert_eq!(json_claim_count(dir.path(), name), 45);

    assert!(store.claim_schedule(name, "fresh-2", 0).await.unwrap());
    assert_eq!(json_claim_count(dir.path(), name), 1);
    let completed = dir
        .path()
        .join(".ironflow-schedule-claim-index-v1")
        .join(".legacy-v1-index-complete");
    assert!(completed.is_file());
}

fn seed_expired_json_claims(root: &std::path::Path, name: &str, count: usize) {
    let claims = root.join(".ironflow-schedule-claims-v1");
    let bucket = root
        .join(".ironflow-schedule-claim-index-v1")
        .join(hex::encode(Sha256::digest(name.as_bytes())))
        .join("0000000000000000");
    std::fs::create_dir_all(&claims).unwrap();
    std::fs::create_dir_all(&bucket).unwrap();
    let old_times = std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH);

    for index in 0..count {
        let key = format!("UTC@2000-01-01T00:{index:05}");
        let claim = json_claim_name(name, &key);
        let claim_path = claims.join(&claim);
        std::fs::write(&claim_path, key.as_bytes()).unwrap();
        std::fs::File::options()
            .write(true)
            .open(claim_path)
            .unwrap()
            .set_times(old_times)
            .unwrap();
        let marker = hex::encode(Sha256::digest(key.as_bytes()));
        let marker_path = bucket.join(marker);
        std::fs::write(&marker_path, claim.as_bytes()).unwrap();
        std::fs::File::options()
            .write(true)
            .open(marker_path)
            .unwrap()
            .set_times(old_times)
            .unwrap();
    }
}

fn seed_legacy_json_claims(root: &std::path::Path, name: &str, count: usize) {
    let claims = root.join(".ironflow-schedule-claims-v1");
    std::fs::create_dir_all(&claims).unwrap();
    for index in 0..count {
        let key = format!("UTC@1999-01-01T00:{index:05}");
        std::fs::write(claims.join(json_claim_name(name, &key)), key).unwrap();
    }
}

fn json_claim_count(root: &std::path::Path, name: &str) -> usize {
    let prefix = json_claim_prefix(name);
    std::fs::read_dir(root.join(".ironflow-schedule-claims-v1"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .count()
}

fn json_claim_name(name: &str, key: &str) -> String {
    format!("{}{}", json_claim_prefix(name), hex::encode(key.as_bytes()))
}

fn json_claim_prefix(name: &str) -> String {
    format!(
        ".ironflow-schedule-claim-v1.{}.",
        hex::encode(name.as_bytes())
    )
}

use ironflow::storage::SqlStateStore;

fn sqlite_url(dir: &std::path::Path) -> String {
    format!("sqlite://{}/claims.sqlite?mode=rwc", dir.display())
}

async fn sqlite_store(dir: &std::path::Path) -> SqlStateStore {
    let url = sqlite_url(dir);
    SqlStateStore::new(&url).await.unwrap()
}

#[tokio::test]
async fn sql_store_grants_a_claim_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = sqlite_store(dir.path()).await;

    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
    assert!(
        !store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
    // A different instant of the same schedule is still available.
    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-02T02:00", TTL)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn two_sql_stores_sharing_a_database_race_and_exactly_one_wins() {
    let dir = tempfile::tempdir().unwrap();
    let first = std::sync::Arc::new(sqlite_store(dir.path()).await);
    let url = sqlite_url(dir.path());
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    for index in 0..300 {
        sqlx::query(
            "INSERT INTO ironflow_schedule_claims (name, claim_key, claimed_micros) \
             VALUES ('nightly', ?, 0)",
        )
        .bind(format!("expired-{index}"))
        .execute(&pool)
        .await
        .unwrap();
    }
    pool.close().await;
    let second = std::sync::Arc::new(sqlite_store(dir.path()).await);

    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for store in [first, second] {
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            store
                .claim_schedule("nightly", "UTC@2026-05-01T02:00", 1)
                .await
                .unwrap()
        }));
    }

    let won = futures_util::future::join_all(handles)
        .await
        .into_iter()
        .filter(|result| *result.as_ref().unwrap())
        .count();
    assert_eq!(won, 1, "exactly one process must own an instant");
}

#[tokio::test]
async fn expired_sql_claims_are_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let store = sqlite_store(dir.path()).await;

    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", 0)
            .await
            .unwrap()
    );
    // A zero TTL expires the first claim, so the next call prunes it and the
    // same key becomes grantable again.
    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-02T02:00", 0)
            .await
            .unwrap()
    );
    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", 0)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn reaping_one_schedule_does_not_delete_another_schedules_claim() {
    // Each schedule derives its own TTL from its own grace window. A
    // short-TTL schedule must not reap a long-TTL schedule's still-valid
    // claim, or the long one could re-fire an instant it already ran.
    let dir = tempfile::tempdir().unwrap();
    let store = sqlite_store(dir.path()).await;

    assert!(
        store
            .claim_schedule("long_grace", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
    // A zero-TTL call from a different schedule: it reaps only its own rows.
    assert!(
        store
            .claim_schedule("short_grace", "UTC@2026-05-01T02:00", 0)
            .await
            .unwrap()
    );
    assert!(
        store
            .claim_schedule("short_grace", "UTC@2026-05-02T02:00", 0)
            .await
            .unwrap()
    );

    // The long-grace schedule's claim survived, so its instant cannot re-fire.
    assert!(
        !store
            .claim_schedule("long_grace", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap(),
        "another schedule's reap deleted this claim"
    );
}

#[tokio::test]
async fn sql_cleanup_uses_its_index_cadence_and_bounded_ten_thousand_row_batch() {
    const EXISTING: i64 = 10_001;
    const CLEANUP_BATCH: i64 = 256;

    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_url(dir.path());
    let store = SqlStateStore::new(&url).await.unwrap();
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(
        "WITH digits(d) AS (VALUES (0),(1),(2),(3),(4),(5),(6),(7),(8),(9)), \
         numbers(n) AS (\
           SELECT a.d + 10*b.d + 100*c.d + 1000*d.d + 10000*e.d \
           FROM digits a CROSS JOIN digits b CROSS JOIN digits c \
           CROSS JOIN digits d CROSS JOIN digits e\
         ) \
         INSERT INTO ironflow_schedule_claims (name, claim_key, claimed_micros) \
         SELECT 'high_frequency', printf('claim-%05d', n), 0 \
         FROM numbers WHERE n < 10001",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert!(
        store
            .claim_schedule("high_frequency", "fresh-1", 1)
            .await
            .unwrap()
    );
    assert!(
        store
            .claim_schedule("high_frequency", "fresh-2", 1)
            .await
            .unwrap()
    );

    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ironflow_schedule_claims WHERE name = 'high_frequency'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, EXISTING - CLEANUP_BATCH + 2);

    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_index_info('ironflow_schedule_claims_gc_idx') \
         ORDER BY seqno",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(columns, ["name", "claimed_micros", "claim_key"]);
    pool.close().await;
}

#[tokio::test]
async fn sql_claims_respect_the_configured_table_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}/prefixed.sqlite?mode=rwc", dir.path().display());
    let store = SqlStateStore::new_with_prefix(&url, Some("tenant_a_"))
        .await
        .unwrap();
    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
    assert!(
        !store
            .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
            .await
            .unwrap()
    );
}

#[cfg(feature = "postgres")]
mod postgres_claims {
    use std::sync::Arc;

    use ironflow::storage::StateStore;

    use super::TTL;
    use super::postgres_support::PostgresStateTest;

    #[tokio::test]
    async fn postgres_claims_are_unique_scoped_and_expire() {
        let Some(fixture) = PostgresStateTest::from_env("pg_schedule_contract") else {
            return;
        };
        let result: anyhow::Result<()> = async {
            let store = fixture.state_store().await?;

            anyhow::ensure!(
                store
                    .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
                    .await?
            );
            anyhow::ensure!(
                !store
                    .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
                    .await?
            );
            anyhow::ensure!(
                store
                    .claim_schedule("nightly", "UTC@2026-05-02T02:00", TTL)
                    .await?
            );
            anyhow::ensure!(
                store
                    .claim_schedule("hourly", "UTC@2026-05-01T02:00", TTL)
                    .await?
            );

            anyhow::ensure!(
                store
                    .claim_schedule("short_grace", "UTC@2026-05-01T02:00", 0)
                    .await?
            );
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            anyhow::ensure!(
                store
                    .claim_schedule("short_grace", "UTC@2026-05-02T02:00", 0)
                    .await?
            );
            anyhow::ensure!(
                store
                    .claim_schedule("short_grace", "UTC@2026-05-01T02:00", 0)
                    .await?,
                "expired PostgreSQL claim was not reaped"
            );
            anyhow::ensure!(
                !store
                    .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
                    .await?,
                "reaping one schedule deleted another schedule's live claim"
            );
            drop(store);
            Ok(())
        }
        .await;

        fixture.cleanup().await.unwrap();
        result.unwrap();
    }

    #[tokio::test]
    async fn two_postgres_stores_race_and_exactly_one_wins() {
        let Some(fixture) = PostgresStateTest::from_env("pg_schedule_race") else {
            return;
        };
        let result: anyhow::Result<()> = async {
            let first = Arc::new(fixture.state_store().await?);
            let second = Arc::new(fixture.state_store().await?);
            let barrier = Arc::new(tokio::sync::Barrier::new(2));
            let mut handles = Vec::new();
            for store in [first, second] {
                let barrier = Arc::clone(&barrier);
                handles.push(tokio::spawn(async move {
                    barrier.wait().await;
                    store
                        .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
                        .await
                }));
            }

            let mut won = 0;
            for handle in handles {
                if handle.await?? {
                    won += 1;
                }
            }
            anyhow::ensure!(
                won == 1,
                "exactly one PostgreSQL replica must win; got {won}"
            );
            Ok(())
        }
        .await;

        fixture.cleanup().await.unwrap();
        result.unwrap();
    }
}

#[cfg(feature = "redis")]
mod redis_claims {
    use std::sync::Arc;

    use ironflow::storage::StateStore;

    use super::TTL;
    use super::redis_support::RedisTest;

    #[tokio::test]
    async fn redis_store_grants_a_claim_once() {
        let Some(fixture) = RedisTest::connect("schedule_claims_once").await else {
            return;
        };
        let store = fixture.state_store(None).await;

        assert!(
            store
                .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
                .await
                .unwrap()
        );
        assert!(
            !store
                .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
                .await
                .unwrap()
        );
        // A different instant of the same schedule is still available.
        assert!(
            store
                .claim_schedule("nightly", "UTC@2026-05-02T02:00", TTL)
                .await
                .unwrap()
        );
        // As is the same instant of a different schedule.
        assert!(
            store
                .claim_schedule("hourly", "UTC@2026-05-01T02:00", TTL)
                .await
                .unwrap()
        );

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn two_redis_stores_race_and_exactly_one_wins() {
        let Some(fixture) = RedisTest::connect("schedule_claims_race").await else {
            return;
        };
        // Two stores on one prefix stand in for two replicas sharing a store.
        let first = Arc::new(fixture.state_store(None).await);
        let second = Arc::new(fixture.state_store(None).await);

        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let mut handles = Vec::new();
        for store in [first, second] {
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                store
                    .claim_schedule("nightly", "UTC@2026-05-01T02:00", TTL)
                    .await
                    .unwrap()
            }));
        }

        let mut won = 0;
        for handle in handles {
            if handle.await.unwrap() {
                won += 1;
            }
        }
        assert_eq!(won, 1, "exactly one replica must own an instant");

        fixture.cleanup().await;
    }

    #[tokio::test]
    async fn redis_claims_expire_on_their_own_ttl() {
        let Some(fixture) = RedisTest::connect("schedule_claims_ttl").await else {
            return;
        };
        let store = fixture.state_store(None).await;

        // Unlike the JSON and SQL backends, Redis retires a claim through the
        // key's own expiry, so no sweep is needed and one schedule's TTL can
        // never shorten another's.
        assert!(
            store
                .claim_schedule("nightly", "UTC@2026-05-01T02:00", 1)
                .await
                .unwrap()
        );
        assert!(
            !store
                .claim_schedule("nightly", "UTC@2026-05-01T02:00", 1)
                .await
                .unwrap()
        );

        tokio::time::sleep(std::time::Duration::from_millis(1_500)).await;
        assert!(
            store
                .claim_schedule("nightly", "UTC@2026-05-01T02:00", 1)
                .await
                .unwrap(),
            "the claim should have expired with its key"
        );

        fixture.cleanup().await;
    }
}
