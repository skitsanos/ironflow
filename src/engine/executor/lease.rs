use std::sync::Arc;

use tokio::sync::{oneshot, watch};

use super::signal::{ExecutionSignal, report_infrastructure};
use crate::engine::types::{Context, RunStatus, TaskState};
use crate::storage::{RunLease, StateStore, StorageError, StorageResult};

/// Bound individual persistence calls below the lease refresh interval. This
/// prevents a partially unavailable backend (for example, writes hanging while
/// lease renewal still succeeds) from pinning a coordinator indefinitely.
pub(super) const STATE_OPERATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone, Copy)]
pub(super) struct HeartbeatTiming {
    refresh: std::time::Duration,
    renewal_timeout: std::time::Duration,
}

impl HeartbeatTiming {
    pub(super) const fn new(
        refresh: std::time::Duration,
        renewal_timeout: std::time::Duration,
    ) -> Self {
        Self {
            refresh,
            renewal_timeout,
        }
    }
}

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
    timing: HeartbeatTiming,
    metrics: Option<Arc<crate::metrics::Metrics>>,
) {
    let start = tokio::time::Instant::now() + timing.refresh;
    let mut ticker = tokio::time::interval_at(start, timing.refresh);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = &mut stop => return,
            _ = ticker.tick() => {
                let lease = RunLease::renewed(owner.clone());
                let renewal = tokio::time::timeout(
                    timing.renewal_timeout,
                    store.renew_run_lease(&run_id, &lease),
                );
                let result = tokio::select! {
                    _ = &mut stop => return,
                    result = renewal => result,
                };
                match result {
                    Err(_) => {
                        if let Some(metrics) = &metrics {
                            metrics.lease(crate::metrics::LeaseOutcome::TimedOut);
                        }
                        tracing::error!(%run_id, "workflow run lease refresh timed out; stopping local execution");
                        report_infrastructure(
                            &cancel,
                            "workflow execution stopped because its ownership lease renewal timed out",
                        );
                        return;
                    }
                    Ok(Ok(true)) => {
                        if let Some(metrics) = &metrics {
                            metrics.lease(crate::metrics::LeaseOutcome::Renewed);
                        }
                    }
                    Ok(Ok(false)) => {
                        if let Some(metrics) = &metrics {
                            metrics.lease(crate::metrics::LeaseOutcome::Lost);
                        }
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
                            if let Some(metrics) = &metrics {
                                metrics.lease(crate::metrics::LeaseOutcome::ReconciliationFailed);
                            }
                            tracing::warn!(%run_id, %error, "failed to reconcile an expired run lease");
                        }
                        return;
                    }
                    Ok(Err(error)) => {
                        if let Some(metrics) = &metrics {
                            metrics.lease(crate::metrics::LeaseOutcome::Error);
                        }
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
mod tests;
