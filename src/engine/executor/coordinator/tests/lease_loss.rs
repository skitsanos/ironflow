use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Notify;

use super::super::*;
use crate::engine::types::{
    Context, FlowDefinition, NodeOutput, RetryConfig, RunInfo, RunStatus, StepDefinition,
    TaskState, TaskStatus,
};
use crate::nodes::{Node, NodeRegistry};
use crate::storage::json_store::JsonStateStore;
use crate::storage::{RunLease, RunListQuery, RunSummaryPage, StateStore, StorageResult};

struct StartedPendingNode {
    started: Arc<Notify>,
}

#[async_trait]
impl Node for StartedPendingNode {
    fn node_type(&self) -> &str {
        "test_started_pending"
    }

    fn description(&self) -> &str {
        "Signal execution start and remain pending"
    }

    async fn execute(&self, _: &serde_json::Value, _: &Context) -> Result<NodeOutput> {
        self.started.notify_one();
        std::future::pending().await
    }
}

struct LeaseLossStore {
    inner: Arc<JsonStateStore>,
    node_started: Arc<Notify>,
    lease_path: PathBuf,
}

#[async_trait]
impl StateStore for LeaseLossStore {
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
        self.inner.set_run_status_owned(run_id, status, owner).await
    }

    async fn renew_run_lease(&self, run_id: &str, lease: &RunLease) -> StorageResult<bool> {
        self.node_started.notified().await;
        tokio::fs::write(
            &self.lease_path,
            serde_json::to_vec(&serde_json::json!({
                "run_id": run_id,
                "owner": lease.owner(),
                "expires_micros": 0,
            }))
            .expect("serialize expired test lease"),
        )
        .await
        .expect("write expired test lease");
        self.inner.renew_run_lease(run_id, lease).await
    }

    async fn reconcile_expired_run_leases(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> StorageResult<usize> {
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

#[tokio::test]
async fn lost_lease_stops_as_infrastructure_and_is_durably_stalled() {
    let directory = tempfile::tempdir().unwrap();
    let inner = Arc::new(JsonStateStore::new(directory.path()));
    let run_id = "lost-lease";
    let owner = "lost-lease-owner";
    let lease = RunLease::renewed(owner.to_string());
    inner
        .init_run_owned(run_id, "lease-loss", &Context::new(), &lease)
        .await
        .unwrap();

    let node_started = Arc::new(Notify::new());
    let store = Arc::new(LeaseLossStore {
        inner: inner.clone(),
        node_started: node_started.clone(),
        lease_path: directory
            .path()
            .join(".ironflow-run-leases-v1/lost-lease.lease"),
    });
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(StartedPendingNode {
        started: node_started,
    }));
    let flow = FlowDefinition {
        name: "lease-loss".to_string(),
        steps: vec![StepDefinition {
            name: "wait".to_string(),
            node_type: "test_started_pending".to_string(),
            config: serde_json::Value::Null,
            dependencies: Vec::new(),
            retry: RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        }],
    };
    let execution_plan = ExecutionPlan::build(&flow).unwrap();
    let coordinator = RunCoordinator::new(
        Arc::new(registry),
        store,
        None,
        1,
        run_id.to_string(),
        flow,
        execution_plan,
        Context::new(),
        ExecutionOverlay::default(),
        owner.to_string(),
        None,
    )
    .with_heartbeat_timing(
        std::time::Duration::from_millis(1),
        std::time::Duration::from_secs(30),
    );

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(40),
        coordinator.spawn().wait(),
    )
    .await
    .expect("lease loss did not settle the run");
    assert!(
        result.is_err(),
        "infrastructure stop must surface to the waiter"
    );
    let info = inner.get_run_info(run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Stalled);
    assert!(info.finished.is_some());
    assert_eq!(info.tasks["wait"].status, TaskStatus::Failed);
    assert!(info.tasks["wait"].finished.is_some());
}
