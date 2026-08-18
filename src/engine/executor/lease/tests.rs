use std::sync::Arc;

use async_trait::async_trait;

use super::*;
use crate::engine::types::{RunInfo, RunStatus};
use crate::storage::{RunListQuery, RunSummaryPage};

#[derive(Clone, Copy)]
enum RenewalBehavior {
    Hang,
    Error,
}

struct RenewalStore(RenewalBehavior);

#[async_trait]
impl StateStore for RenewalStore {
    async fn init_run(&self, _: &str, _: &str, _: &Context) -> StorageResult<()> {
        std::future::pending().await
    }

    async fn set_run_status(&self, _: &str, _: RunStatus) -> StorageResult<()> {
        unreachable!()
    }

    async fn renew_run_lease(&self, _: &str, _: &RunLease) -> StorageResult<bool> {
        match self.0 {
            RenewalBehavior::Hang => std::future::pending().await,
            RenewalBehavior::Error => Err(StorageError::backend(
                "renew test lease",
                "injected failure",
            )),
        }
    }

    async fn upsert_task(&self, _: &str, _: &TaskState) -> StorageResult<()> {
        unreachable!()
    }

    async fn get_ctx(&self, _: &str) -> StorageResult<Context> {
        unreachable!()
    }

    async fn update_ctx(&self, _: &str, _: &Context) -> StorageResult<()> {
        unreachable!()
    }

    async fn get_run_info(&self, _: &str) -> StorageResult<RunInfo> {
        unreachable!()
    }

    async fn list_runs(&self, _: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        unreachable!()
    }

    async fn list_run_summaries_page(&self, _: &RunListQuery) -> StorageResult<RunSummaryPage> {
        unreachable!()
    }

    async fn delete_run(&self, _: &str) -> StorageResult<()> {
        unreachable!()
    }
}

#[tokio::test]
async fn a_hung_initial_write_is_bounded() {
    let store = RenewalStore(RenewalBehavior::Hang);
    let ctx = Context::new();
    let lease = RunLease::fresh();
    let error = bounded(
        "initialize workflow run",
        store.init_run_owned("run", "flow", &ctx, &lease),
        std::time::Duration::from_millis(5),
    )
    .await
    .unwrap_err();

    assert_eq!(error.kind(), crate::storage::StorageErrorKind::Backend);
    assert!(error.to_string().contains("timed out"));
}

#[tokio::test]
async fn a_hung_renewal_is_bounded_and_reports_infrastructure() {
    let (cancel, mut execution_signal) = watch::channel(ExecutionSignal::Running);
    let (_stop, heartbeat_stop) = oneshot::channel();
    let metrics = Arc::new(crate::metrics::Metrics::new());
    let heartbeat = tokio::spawn(heartbeat_with_timing(
        Arc::new(RenewalStore(RenewalBehavior::Hang)),
        "run".to_string(),
        "owner".to_string(),
        cancel,
        heartbeat_stop,
        HeartbeatTiming::new(
            std::time::Duration::from_millis(1),
            std::time::Duration::from_millis(5),
        ),
        Some(metrics.clone()),
    ));

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        execution_signal.changed(),
    )
    .await
    .expect("hung lease renewal did not stop execution within its timeout")
    .unwrap();
    assert!(matches!(
        &*execution_signal.borrow(),
        ExecutionSignal::Infrastructure(reason) if reason.contains("timed out")
    ));
    tokio::time::timeout(std::time::Duration::from_secs(1), heartbeat)
        .await
        .expect("heartbeat did not terminate after renewal timeout")
        .unwrap();
    assert!(
        metrics
            .encode()
            .unwrap()
            .contains("ironflow_lease_events_total{outcome=\"timed_out\"} 1")
    );
}

#[tokio::test]
async fn a_renewal_error_reports_infrastructure() {
    let (signal, mut execution_signal) = watch::channel(ExecutionSignal::Running);
    let (_stop, heartbeat_stop) = oneshot::channel();
    let metrics = Arc::new(crate::metrics::Metrics::new());
    heartbeat_with_timing(
        Arc::new(RenewalStore(RenewalBehavior::Error)),
        "run".to_string(),
        "owner".to_string(),
        signal,
        heartbeat_stop,
        HeartbeatTiming::new(
            std::time::Duration::from_millis(1),
            std::time::Duration::from_millis(50),
        ),
        Some(metrics.clone()),
    )
    .await;

    execution_signal.changed().await.unwrap();
    assert!(matches!(
        &*execution_signal.borrow(),
        ExecutionSignal::Infrastructure(reason) if reason.contains("injected failure")
    ));
    assert!(
        metrics
            .encode()
            .unwrap()
            .contains("ironflow_lease_events_total{outcome=\"error\"} 1")
    );
}
