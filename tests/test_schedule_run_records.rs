//! Store contract for the "run created" record of a schedule claim (IF-201):
//! `record_schedule_run` and `schedule_run_recorded` on every built-in store.

use ironflow::storage::json_store::JsonStateStore;
use ironflow::storage::null_store::NullStateStore;
use ironflow::storage::{SqlStateStore, StateStore};

const TTL: u64 = 7 * 24 * 3600;
const KEY: &str = "UTC@2026-05-01T02:00";

#[cfg(feature = "redis")]
#[path = "support/redis.rs"]
mod redis_support;

#[cfg(feature = "postgres")]
#[path = "support/postgres.rs"]
mod postgres_support;

/// What the scheduler relies on, identical on every shared store.
async fn run_record_contract(store: &dyn StateStore) -> anyhow::Result<()> {
    anyhow::ensure!(
        !store.schedule_run_recorded("nightly", KEY).await?,
        "an unknown occurrence must not read as recorded"
    );

    anyhow::ensure!(store.claim_schedule("nightly", KEY, TTL).await?);
    anyhow::ensure!(
        !store.schedule_run_recorded("nightly", KEY).await?,
        "a claim that produced no run is an orphan, not a recorded run"
    );

    store.record_schedule_run("nightly", KEY, TTL).await?;
    anyhow::ensure!(store.schedule_run_recorded("nightly", KEY).await?);
    // Both the starting replica and a converging peer record the run.
    store.record_schedule_run("nightly", KEY, TTL).await?;
    anyhow::ensure!(store.schedule_run_recorded("nightly", KEY).await?);

    anyhow::ensure!(
        !store.claim_schedule("nightly", KEY, TTL).await?,
        "recording the run must not release the claim"
    );
    anyhow::ensure!(
        !store
            .schedule_run_recorded("nightly", "UTC@2026-05-02T02:00")
            .await?,
        "another instant of the schedule is not recorded"
    );
    anyhow::ensure!(
        !store.schedule_run_recorded("hourly", KEY).await?,
        "the same instant of another schedule is not recorded"
    );
    // A key that looks like a marked value of another key is still its own
    // occurrence.
    let lookalike = format!("run:{KEY}");
    anyhow::ensure!(store.claim_schedule("nightly", &lookalike, TTL).await?);
    anyhow::ensure!(!store.schedule_run_recorded("nightly", &lookalike).await?);
    Ok(())
}

#[tokio::test]
async fn the_null_store_records_nothing() {
    // Process-local and non-persistent: there is no later replica to fence.
    let store = NullStateStore::new();
    store
        .record_schedule_run("nightly", KEY, TTL)
        .await
        .unwrap();
    assert!(!store.schedule_run_recorded("nightly", KEY).await.unwrap());
}

#[tokio::test]
async fn json_store_keeps_the_run_record_contract() {
    let dir = tempfile::tempdir().unwrap();
    run_record_contract(&JsonStateStore::new(dir.path()))
        .await
        .unwrap();
}

#[tokio::test]
async fn json_run_records_are_reaped_with_their_claims() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());
    let files = |prefix: &'static str| {
        std::fs::read_dir(dir.path().join(".ironflow-schedule-claims-v1"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
            .count()
    };

    // A zero TTL makes the claim immediately expired.
    assert!(store.claim_schedule("nightly", KEY, 0).await.unwrap());
    store.record_schedule_run("nightly", KEY, 0).await.unwrap();
    assert_eq!(files(".ironflow-schedule-run-v1."), 1);

    // The next claim of the schedule reaps the expired one and its record.
    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-02T02:00", 0)
            .await
            .unwrap()
    );
    assert_eq!(files(".ironflow-schedule-run-v1."), 0);
    assert_eq!(files(".ironflow-schedule-claim-v1."), 1);
    assert!(!store.schedule_run_recorded("nightly", KEY).await.unwrap());
}

fn sqlite_url(dir: &std::path::Path) -> String {
    format!("sqlite://{}/records.sqlite?mode=rwc", dir.display())
}

#[tokio::test]
async fn sqlite_store_keeps_the_run_record_contract() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqlStateStore::new(&sqlite_url(dir.path())).await.unwrap();
    run_record_contract(&store).await.unwrap();
}

#[tokio::test]
async fn sql_run_records_are_reaped_with_their_claims() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqlStateStore::new(&sqlite_url(dir.path())).await.unwrap();

    assert!(store.claim_schedule("nightly", KEY, 0).await.unwrap());
    store.record_schedule_run("nightly", KEY, 0).await.unwrap();
    assert!(store.schedule_run_recorded("nightly", KEY).await.unwrap());

    assert!(
        store
            .claim_schedule("nightly", "UTC@2026-05-02T02:00", 0)
            .await
            .unwrap()
    );
    assert!(!store.schedule_run_recorded("nightly", KEY).await.unwrap());
}

/// A claims table created before IF-201 has no `run_micros` column. Opening
/// the store adds it, and claims written by the older binary read as orphans.
#[tokio::test]
async fn a_claims_table_from_an_older_binary_is_upgraded_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let url = sqlite_url(dir.path());
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TABLE ironflow_schedule_claims (\
         name TEXT NOT NULL, claim_key TEXT NOT NULL, claimed_micros BIGINT NOT NULL, \
         PRIMARY KEY (name, claim_key))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ironflow_schedule_claims (name, claim_key, claimed_micros) \
         VALUES ('nightly', ?, ?)",
    )
    .bind(KEY)
    .bind(chrono::Utc::now().timestamp_micros())
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let store = SqlStateStore::new(&url).await.unwrap();
    // Two replicas of the new binary can start together.
    let peer = SqlStateStore::new(&url).await.unwrap();

    assert!(!store.claim_schedule("nightly", KEY, TTL).await.unwrap());
    assert!(!store.schedule_run_recorded("nightly", KEY).await.unwrap());
    store
        .record_schedule_run("nightly", KEY, TTL)
        .await
        .unwrap();
    assert!(peer.schedule_run_recorded("nightly", KEY).await.unwrap());
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn postgres_store_keeps_the_run_record_contract() {
    let Some(fixture) = postgres_support::PostgresStateTest::from_env("pg_run_record") else {
        return;
    };
    let result: anyhow::Result<()> = async {
        let store = fixture.state_store().await?;
        run_record_contract(&store).await?;
        // A second store on the same tables finds the column already there.
        let peer = fixture.state_store().await?;
        anyhow::ensure!(peer.schedule_run_recorded("nightly", KEY).await?);
        Ok(())
    }
    .await;
    fixture.cleanup().await.unwrap();
    result.unwrap();
}

#[cfg(feature = "redis")]
mod redis_records {
    use std::time::Duration;

    use ironflow::storage::StateStore;
    use redis::AsyncCommands as _;

    use super::redis_support::RedisTest;
    use super::{KEY, TTL};

    #[tokio::test]
    async fn redis_store_keeps_the_run_record_contract() {
        let Some(fixture) = RedisTest::connect("run_record_contract").await else {
            return;
        };
        let store = fixture.state_store(None).await;
        let result = super::run_record_contract(&store).await;
        fixture.cleanup().await;
        result.unwrap();
    }

    /// The record's lifetime is the claim's, not `REDIS_TTL`: a store whose
    /// run records expire after one second keeps the record for the claim TTL.
    #[tokio::test]
    async fn the_redis_run_record_lives_as_long_as_the_claim() {
        let Some(fixture) = RedisTest::connect("run_record_ttl").await else {
            return;
        };
        let store = fixture.state_store(Some(1)).await;
        assert!(store.claim_schedule("nightly", KEY, TTL).await.unwrap());
        store
            .record_schedule_run("nightly", KEY, TTL)
            .await
            .unwrap();

        let mut conn = fixture.connection().await.unwrap();
        let keys: Vec<String> = conn
            .keys(format!("{}schedule_claims:*", fixture.prefix))
            .await
            .unwrap();
        let remaining: i64 = conn.ttl(&keys[0]).await.unwrap();
        fixture.cleanup().await;

        assert_eq!(keys.len(), 1, "the record lives in the claim key itself");
        assert!(
            remaining > 86_400 && remaining <= i64::try_from(TTL).unwrap(),
            "the record must keep the claim lifetime, got {remaining}s"
        );
    }

    /// The record expires with the claim key. State is polled, so a loaded
    /// machine can only make this slower (IF-216).
    #[tokio::test]
    async fn the_redis_run_record_expires_with_the_claim() {
        let Some(fixture) = RedisTest::connect("run_record_expiry").await else {
            return;
        };
        let store = fixture.state_store(None).await;
        assert!(store.claim_schedule("nightly", KEY, TTL).await.unwrap());
        store.record_schedule_run("nightly", KEY, 1).await.unwrap();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        let mut expired = false;
        while tokio::time::Instant::now() < deadline {
            if !store.schedule_run_recorded("nightly", KEY).await.unwrap() {
                expired = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let reclaimed = store.claim_schedule("nightly", KEY, TTL).await.unwrap();
        fixture.cleanup().await;

        assert!(expired, "the record must expire with its key");
        assert!(reclaimed, "an expired record leaves the instant claimable");
    }
}
