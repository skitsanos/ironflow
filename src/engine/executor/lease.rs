use std::sync::Arc;

use tokio::sync::{oneshot, watch};

use super::signal::{ExecutionSignal, report_infrastructure};
use crate::engine::types::{Context, RunStatus, TaskState};
use crate::storage::{RunLease, StateStore, StorageError, StorageResult};

/// Bound individual persistence calls below the lease refresh interval. This
/// prevents a partially unavailable backend (for example, writes hanging while
/// lease renewal still succeeds) from pinning a coordinator indefinitely.
pub(super) const STATE_OPERATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub(super) async fn initialize_run(
    store: &dyn StateStore,
    run_id: &str,
    flow_name: &str,
    ctx: &Context,
    lease: &RunLease,
) -> StorageResult<()> {
    bounded(
        "initialize workflow run",
        store.init_run_owned(run_id, flow_name, ctx, lease),
        STATE_OPERATION_TIMEOUT,
    )
    .await
}
pub(super) async fn persist_task(
    store: &dyn StateStore,
    run_id: &str,
    task: &TaskState,
    owner: &str,
) -> StorageResult<()> {
    if bounded(
        "persist workflow task",
        store.upsert_task_owned(run_id, task, owner),
        STATE_OPERATION_TIMEOUT,
    )
    .await?
    {
        Ok(())
    } else {
        Err(StorageError::conflict(format_args!(
            "Run '{run_id}' lost its execution-owner lease"
        )))
    }
}

pub(super) async fn persist_context(
    store: &dyn StateStore,
    run_id: &str,
    ctx: &Context,
    owner: &str,
) -> StorageResult<()> {
    if bounded(
        "persist workflow context",
        store.update_ctx_owned(run_id, ctx, owner),
        STATE_OPERATION_TIMEOUT,
    )
    .await?
    {
        Ok(())
    } else {
        Err(StorageError::conflict(format_args!(
            "Run '{run_id}' lost its execution-owner lease"
        )))
    }
}

pub(super) async fn persist_status(
    store: &dyn StateStore,
    run_id: &str,
    status: RunStatus,
    owner: &str,
) -> StorageResult<bool> {
    bounded(
        "persist workflow status",
        store.set_run_status_owned(run_id, status, owner),
        STATE_OPERATION_TIMEOUT,
    )
    .await
}

async fn bounded<T>(
    operation: &'static str,
    future: impl std::future::Future<Output = StorageResult<T>>,
    timeout: std::time::Duration,
) -> StorageResult<T> {
    tokio::time::timeout(timeout, future).await.map_err(|_| {
        StorageError::backend(
            operation,
            format_args!("operation timed out after {}ms", timeout.as_millis()),
        )
    })?
}
/// Keep one coordinator's durable ownership live until supervision finishes.
/// Any refresh failure reports an infrastructure stop rather than allowing
/// work to continue beyond a lease another replica may reconcile.
pub(super) async fn heartbeat_with_timing(
    store: Arc<dyn StateStore>,
    run_id: String,
    owner: String,
    cancel: watch::Sender<ExecutionSignal>,
    mut stop: oneshot::Receiver<()>,
    refresh: std::time::Duration,
    renewal_timeout: std::time::Duration,
) {
    let start = tokio::time::Instant::now() + refresh;
    let mut ticker = tokio::time::interval_at(start, refresh);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = &mut stop => return,
            _ = ticker.tick() => {
                let lease = RunLease::renewed(owner.clone());
                let renewal = tokio::time::timeout(
                    renewal_timeout,
                    store.renew_run_lease(&run_id, &lease),
                );
                let result = tokio::select! {
                    _ = &mut stop => return,
                    result = renewal => result,
                };
                match result {
                    Err(_) => {
                        tracing::error!(%run_id, "workflow run lease refresh timed out; stopping local execution");
                        report_infrastructure(
                            &cancel,
                            "workflow execution stopped because its ownership lease renewal timed out",
                        );
                        return;
                    }
                    Ok(Ok(true)) => {}
                    Ok(Ok(false)) => {
                        tracing::error!(%run_id, "workflow run ownership lease expired or was lost; stopping local execution");
                        report_infrastructure(
                            &cancel,
                            "workflow execution stopped after its ownership lease was lost",
                        );
                        if let Err(error) = bounded(
                            "reconcile an expired workflow run lease",
                            store.reconcile_expired_run_leases(chrono::Utc::now()),
                            STATE_OPERATION_TIMEOUT,
                        ).await {
                            tracing::warn!(%run_id, %error, "failed to reconcile an expired run lease");
                        }
                        return;
                    }
                    Ok(Err(error)) => {
                        tracing::error!(%run_id, %error, "workflow run lease refresh failed; stopping local execution");
                        report_infrastructure(
                            &cancel,
                            format!("workflow execution stopped because ownership lease renewal failed: {error}"),
                        );
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
        let heartbeat = tokio::spawn(heartbeat_with_timing(
            Arc::new(RenewalStore(RenewalBehavior::Hang)),
            "run".to_string(),
            "owner".to_string(),
            cancel,
            heartbeat_stop,
            std::time::Duration::from_millis(1),
            std::time::Duration::from_millis(5),
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
    }

    #[tokio::test]
    async fn a_renewal_error_reports_infrastructure() {
        let (signal, mut execution_signal) = watch::channel(ExecutionSignal::Running);
        let (_stop, heartbeat_stop) = oneshot::channel();
        heartbeat_with_timing(
            Arc::new(RenewalStore(RenewalBehavior::Error)),
            "run".to_string(),
            "owner".to_string(),
            signal,
            heartbeat_stop,
            std::time::Duration::from_millis(1),
            std::time::Duration::from_millis(50),
        )
        .await;

        execution_signal.changed().await.unwrap();
        assert!(matches!(
            &*execution_signal.borrow(),
            ExecutionSignal::Infrastructure(reason) if reason.contains("injected failure")
        ));
    }
}
