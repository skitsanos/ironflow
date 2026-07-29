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
