//! IF-203 (d): a heartbeat renewal that lands after the run's own terminal
//! status write finds the lease released. That is the normal end of a run,
//! not a lost lease.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Notify;

use super::super::*;
use crate::engine::types::{
    Context, FlowDefinition, NodeOutput, RetryConfig, RunInfo, RunStatus, StepDefinition, TaskState,
};
use crate::nodes::{Node, NodeRegistry};
use crate::storage::{RunLease, RunListQuery, RunSummaryPage, StateStore, StorageResult};

struct DoneNode;

#[async_trait]
impl Node for DoneNode {
    fn node_type(&self) -> &str {
        "test_done"
    }

    fn description(&self) -> &str {
        "Finish at once"
    }

    async fn execute(&self, _: &serde_json::Value, _: &Context) -> Result<NodeOutput> {
        Ok(NodeOutput::new())
    }
}

/// Holds the terminal status write open until one more renewal has run, which
/// is the window between that write and the heartbeat stop.
struct LateRenewalStore {
    inner: Arc<dyn StateStore>,
    late_renewal: Notify,
    late_renewals_refused: AtomicUsize,
    reconciliations: AtomicUsize,
}

#[async_trait]
impl StateStore for LateRenewalStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()> {
        self.inner.init_run(run_id, flow_name, ctx).await
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        self.inner.set_run_status(run_id, status).await
    }

    async fn set_run_status_owned(
        &self,
        run_id: &str,
        status: RunStatus,
        owner: &str,
    ) -> StorageResult<bool> {
        let terminal = status.is_terminal();
        let written = self
            .inner
            .set_run_status_owned(run_id, status, owner)
            .await?;
        if terminal && written {
            // `notify_one` keeps a permit, so a renewal that was refused
            // before this line is not missed.
            self.late_renewal.notified().await;
        }
        Ok(written)
    }

    async fn renew_run_lease(&self, run_id: &str, lease: &RunLease) -> StorageResult<bool> {
        let renewed = self.inner.renew_run_lease(run_id, lease).await?;
        // Nothing else in this test takes the lease away, so a refusal is the
        // renewal that followed the terminal write.
        if !renewed {
            self.late_renewals_refused.fetch_add(1, Ordering::SeqCst);
            self.late_renewal.notify_one();
        }
        Ok(renewed)
    }

    async fn reconcile_expired_run_leases(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> StorageResult<usize> {
        self.reconciliations.fetch_add(1, Ordering::SeqCst);
        self.inner.reconcile_expired_run_leases(now).await
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()> {
        self.inner.upsert_task(run_id, task).await
    }

    async fn upsert_task_owned(
        &self,
        run_id: &str,
        task: &TaskState,
        owner: &str,
    ) -> StorageResult<bool> {
        self.inner.upsert_task_owned(run_id, task, owner).await
    }

    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context> {
        self.inner.get_ctx(run_id).await
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()> {
        self.inner.update_ctx(run_id, ctx).await
    }

    async fn update_ctx_owned(
        &self,
        run_id: &str,
        ctx: &Context,
        owner: &str,
    ) -> StorageResult<bool> {
        self.inner.update_ctx_owned(run_id, ctx, owner).await
    }

    async fn get_run_info(&self, run_id: &str) -> StorageResult<RunInfo> {
        self.inner.get_run_info(run_id).await
    }

    async fn list_runs(&self, status: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        self.inner.list_runs(status).await
    }

    async fn list_run_summaries_page(&self, query: &RunListQuery) -> StorageResult<RunSummaryPage> {
        self.inner.list_run_summaries_page(query).await
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<()> {
        self.inner.delete_run(run_id).await
    }
}

async fn finish_with_a_late_renewal(inner: Arc<dyn StateStore>) {
    let run_id = "late-renewal";
    let owner = "late-renewal-owner";
    inner
        .init_run_owned(
            run_id,
            "late-renewal",
            &Context::new(),
            &RunLease::renewed(owner.to_string()),
        )
        .await
        .unwrap();
    let store = Arc::new(LateRenewalStore {
        inner: inner.clone(),
        late_renewal: Notify::new(),
        late_renewals_refused: AtomicUsize::new(0),
        reconciliations: AtomicUsize::new(0),
    });
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(DoneNode));
    let flow = FlowDefinition {
        name: "late-renewal".to_string(),
        steps: vec![StepDefinition {
            name: "done".to_string(),
            node_type: "test_done".to_string(),
            config: serde_json::Value::Null,
            dependencies: Vec::new(),
            retry: RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        }],
    };
    let execution_plan = ExecutionPlan::build(&flow).unwrap();
    let metrics = Arc::new(crate::metrics::Metrics::new());
    let coordinator = RunCoordinator::new(
        Arc::new(registry),
        store.clone(),
        None,
        1,
        run_id.to_string(),
        flow,
        execution_plan,
        Context::new(),
        ExecutionOverlay::default(),
        owner.to_string(),
        None,
        Some(metrics.clone()),
    )
    .with_heartbeat_timing(
        std::time::Duration::from_millis(1),
        std::time::Duration::from_secs(30),
    );

    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        coordinator.spawn().wait(),
    )
    .await
    .expect("the run did not settle")
    .expect("the run succeeds");

    assert_eq!(
        inner.get_run_info(run_id).await.unwrap().status,
        RunStatus::Success
    );
    // The window is real: the store refused the renewal that followed the
    // terminal write.
    assert_eq!(store.late_renewals_refused.load(Ordering::SeqCst), 1);
    assert_eq!(
        store.reconciliations.load(Ordering::SeqCst),
        0,
        "a finished run must not start a lease reconciliation pass"
    );
    let exposition = metrics.encode().unwrap();
    let lost = exposition
        .lines()
        .find(|line| line.contains("lease_events") && line.contains("outcome=\"lost\""))
        .unwrap_or_else(|| panic!("no lost-lease series in:\n{exposition}"));
    assert!(lost.ends_with(" 0"), "{lost}");
}

#[tokio::test]
async fn a_renewal_after_the_terminal_write_is_not_a_lost_lease_on_sqlite() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("state.sqlite").display()
    );
    let store = crate::storage::sql_store::SqlStateStore::new(&url)
        .await
        .unwrap();
    finish_with_a_late_renewal(Arc::new(store)).await;
}

#[tokio::test]
async fn a_renewal_after_the_terminal_write_is_not_a_lost_lease_on_json() {
    let directory = tempfile::tempdir().unwrap();
    let store = crate::storage::json_store::JsonStateStore::new(directory.path());
    finish_with_a_late_renewal(Arc::new(store)).await;
}
