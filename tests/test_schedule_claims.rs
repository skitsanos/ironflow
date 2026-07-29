use ironflow::storage::null_store::NullStateStore;
use ironflow::storage::{StateStore, StorageErrorKind};

const TTL: u64 = 7 * 24 * 3600;

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
    let first = std::sync::Arc::new(JsonStateStore::new(dir.path()));
    let second = std::sync::Arc::new(JsonStateStore::new(dir.path()));

    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
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
        std::fs::read_dir(dir.path())
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
