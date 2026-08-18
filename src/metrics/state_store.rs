use std::sync::Arc;

use async_trait::async_trait;

use super::{Metrics, StorageOperation, StoreKind};
use crate::engine::types::{Context, RunInfo, RunStatus, RunSummary, TaskState};
use crate::storage::{RunLease, RunListQuery, RunSummaryPage, StateStore, StorageResult};

pub(crate) fn observe_state_store(
    inner: Arc<dyn StateStore>,
    metrics: Arc<Metrics>,
) -> Arc<dyn StateStore> {
    Arc::new(ObservedStateStore { inner, metrics })
}

struct ObservedStateStore {
    inner: Arc<dyn StateStore>,
    metrics: Arc<Metrics>,
}

impl ObservedStateStore {
    fn observe<T>(
        &self,
        operation: StorageOperation,
        result: StorageResult<T>,
    ) -> StorageResult<T> {
        if let Err(error) = &result {
            self.metrics
                .storage_failure(StoreKind::State, operation, error.kind());
        }
        result
    }
}

#[async_trait]
impl StateStore for ObservedStateStore {
    async fn healthcheck(&self) -> StorageResult<()> {
        let result = self.inner.healthcheck().await;
        self.observe(StorageOperation::Healthcheck, result)
    }

    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()> {
        let result = self.inner.init_run(run_id, flow_name, ctx).await;
        self.observe(StorageOperation::InitRun, result)
    }

    async fn init_run_owned(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
        lease: &RunLease,
    ) -> StorageResult<()> {
        let result = self
            .inner
            .init_run_owned(run_id, flow_name, ctx, lease)
            .await;
        self.observe(StorageOperation::InitRunOwned, result)
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        let result = self.inner.set_run_status(run_id, status).await;
        self.observe(StorageOperation::SetRunStatus, result)
    }

    async fn set_run_status_owned(
        &self,
        run_id: &str,
        status: RunStatus,
        owner: &str,
    ) -> StorageResult<bool> {
        let result = self.inner.set_run_status_owned(run_id, status, owner).await;
        self.observe(StorageOperation::SetRunStatusOwned, result)
    }

    async fn renew_run_lease(&self, run_id: &str, lease: &RunLease) -> StorageResult<bool> {
        let result = self.inner.renew_run_lease(run_id, lease).await;
        self.observe(StorageOperation::RenewRunLease, result)
    }

    async fn reconcile_expired_run_leases(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> StorageResult<usize> {
        let result = self.inner.reconcile_expired_run_leases(now).await;
        self.observe(StorageOperation::ReconcileExpiredRunLeases, result)
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()> {
        let result = self.inner.upsert_task(run_id, task).await;
        self.observe(StorageOperation::UpsertTask, result)
    }

    async fn upsert_task_owned(
        &self,
        run_id: &str,
        task: &TaskState,
        owner: &str,
    ) -> StorageResult<bool> {
        let result = self.inner.upsert_task_owned(run_id, task, owner).await;
        self.observe(StorageOperation::UpsertTaskOwned, result)
    }

    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context> {
        let result = self.inner.get_ctx(run_id).await;
        self.observe(StorageOperation::GetContext, result)
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()> {
        let result = self.inner.update_ctx(run_id, ctx).await;
        self.observe(StorageOperation::UpdateContext, result)
    }

    async fn update_ctx_owned(
        &self,
        run_id: &str,
        ctx: &Context,
        owner: &str,
    ) -> StorageResult<bool> {
        let result = self.inner.update_ctx_owned(run_id, ctx, owner).await;
        self.observe(StorageOperation::UpdateContextOwned, result)
    }

    async fn get_run_info(&self, run_id: &str) -> StorageResult<RunInfo> {
        let result = self.inner.get_run_info(run_id).await;
        self.observe(StorageOperation::GetRunInfo, result)
    }

    async fn list_runs(&self, status: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        let result = self.inner.list_runs(status).await;
        self.observe(StorageOperation::ListRuns, result)
    }

    async fn list_run_summaries(
        &self,
        status: Option<RunStatus>,
    ) -> StorageResult<Vec<RunSummary>> {
        let result = self.inner.list_run_summaries(status).await;
        self.observe(StorageOperation::ListRunSummaries, result)
    }

    async fn list_run_summaries_page(&self, query: &RunListQuery) -> StorageResult<RunSummaryPage> {
        let result = self.inner.list_run_summaries_page(query).await;
        self.observe(StorageOperation::ListRunSummariesPage, result)
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<()> {
        let result = self.inner.delete_run(run_id).await;
        self.observe(StorageOperation::DeleteRun, result)
    }

    async fn prune_before(&self, cutoff: chrono::DateTime<chrono::Utc>) -> StorageResult<usize> {
        let result = self.inner.prune_before(cutoff).await;
        self.observe(StorageOperation::PruneBefore, result)
    }

    async fn claim_schedule(&self, name: &str, key: &str, ttl_seconds: u64) -> StorageResult<bool> {
        let result = self.inner.claim_schedule(name, key, ttl_seconds).await;
        self.observe(StorageOperation::ClaimSchedule, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::null_store::NullStateStore;

    #[tokio::test]
    async fn storage_diagnostics_and_identifiers_never_become_labels() {
        let metrics = Arc::new(Metrics::new());
        let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
        let store = observe_state_store(store, metrics.clone());
        let sentinel = "probe-id";

        store.get_ctx(sentinel).await.unwrap_err();
        let encoded = metrics.encode().unwrap();

        assert!(!encoded.contains(sentinel));
        assert!(encoded.contains(
            "ironflow_storage_failures_total{store=\"state\",operation=\"get_context\",error_kind=\"not_found\"} 1"
        ));
    }
}
