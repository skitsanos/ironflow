use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use futures_util::FutureExt as _;
use tokio::sync::{RwLock, oneshot, watch};
use tracing::error;

use crate::engine::recovery::ExecutionPlan;
use crate::engine::types::{Context, FlowDefinition};
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;
use crate::util::execution::{CooperativeWorkerSet, with_run_worker_set};

use super::overlay::ExecutionOverlay;
use super::signal::{ExecutionOutcome, ExecutionSignal, request_cancellation, stop_requested};

const FINALIZATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Handle for a supervised workflow execution.
///
/// Dropping this value detaches from the run without cancelling it. This keeps
/// HTTP disconnects and cancelled waiters from stranding durable state.
pub struct RunHandle {
    run_id: String,
    cancel: watch::Sender<ExecutionSignal>,
    completion: oneshot::Receiver<Result<()>>,
}

/// Cloneable cancellation authority retained by the service lifecycle while a
/// run is active. It cannot await or otherwise consume the public run handle.
#[derive(Clone)]
pub(crate) struct RunCancellation {
    signal: watch::Sender<ExecutionSignal>,
}

impl RunCancellation {
    pub(crate) fn request(&self) {
        request_cancellation(&self.signal);
    }
}

struct CancelRunOnDrop {
    cancel: Option<watch::Sender<ExecutionSignal>>,
}

impl CancelRunOnDrop {
    fn new(cancel: watch::Sender<ExecutionSignal>) -> Self {
        Self {
            cancel: Some(cancel),
        }
    }

    fn disarm(&mut self) {
        self.cancel = None;
    }
}

impl Drop for CancelRunOnDrop {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            request_cancellation(&cancel);
        }
    }
}

impl RunHandle {
    pub fn id(&self) -> &str {
        &self.run_id
    }

    pub(crate) fn cancellation(&self) -> RunCancellation {
        RunCancellation {
            signal: self.cancel.clone(),
        }
    }

    pub async fn wait(self) -> Result<String> {
        self.finish(false).await
    }

    /// Wait for an internally-owned child run, cancelling it if the awaiting
    /// parent future is dropped. Public `wait` deliberately retains detach
    /// semantics; structured workflow composition uses this stricter variant.
    pub(crate) async fn wait_cancel_on_drop(self) -> Result<String> {
        let mut cancel_on_drop = CancelRunOnDrop::new(self.cancel.clone());
        let result = self.finish(false).await;
        cancel_on_drop.disarm();
        result
    }

    pub async fn cancel(self) -> Result<String> {
        self.finish(true).await
    }

    async fn finish(self, cancel: bool) -> Result<String> {
        if cancel {
            request_cancellation(&self.cancel);
        }

        self.completion.await.with_context(|| {
            format!("Run coordinator for '{}' stopped unexpectedly", self.run_id)
        })??;
        Ok(self.run_id)
    }
}

pub(super) struct RunCoordinator {
    pub(super) registry: Arc<NodeRegistry>,
    pub(super) store: Arc<dyn StateStore>,
    pub(super) events: Option<Arc<dyn EventStore>>,
    pub(super) max_concurrent_tasks: usize,
    pub(super) run_id: String,
    pub(super) flow: FlowDefinition,
    pub(super) execution_plan: ExecutionPlan,
    pub(super) ctx: Arc<RwLock<Arc<Context>>>,
    pub(super) execution_overlay: ExecutionOverlay,
    pub(super) lease_owner: String,
    pub(super) run_deadline: Option<std::time::Duration>,
    finalization_timeout: std::time::Duration,
    heartbeat_refresh: std::time::Duration,
    heartbeat_timeout: std::time::Duration,
}

impl RunCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        registry: Arc<NodeRegistry>,
        store: Arc<dyn StateStore>,
        events: Option<Arc<dyn EventStore>>,
        max_concurrent_tasks: usize,
        run_id: String,
        flow: FlowDefinition,
        execution_plan: ExecutionPlan,
        initial_ctx: Context,
        execution_overlay: ExecutionOverlay,
        lease_owner: String,
        run_deadline: Option<std::time::Duration>,
    ) -> Self {
        Self {
            registry,
            store,
            events,
            max_concurrent_tasks,
            run_id,
            flow,
            execution_plan,
            ctx: Arc::new(RwLock::new(Arc::new(initial_ctx))),
            execution_overlay,
            lease_owner,
            run_deadline,
            finalization_timeout: FINALIZATION_TIMEOUT,
            heartbeat_refresh: crate::storage::RUN_LEASE_REFRESH,
            heartbeat_timeout: crate::storage::RUN_LEASE_REFRESH,
        }
    }

    #[cfg(test)]
    fn with_finalization_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.finalization_timeout = timeout;
        self
    }

    #[cfg(test)]
    fn with_heartbeat_timing(
        mut self,
        refresh: std::time::Duration,
        timeout: std::time::Duration,
    ) -> Self {
        self.heartbeat_refresh = refresh;
        self.heartbeat_timeout = timeout;
        self
    }

    pub(super) fn spawn(self) -> RunHandle {
        let run_id = self.run_id.clone();
        let (cancel, cancel_rx) = watch::channel(ExecutionSignal::Running);
        let cancel_owner = cancel.clone();
        let lease_cancel = cancel.clone();
        let (completion_tx, completion) = oneshot::channel();

        // Optional run-level deadline. When a run outlives it, the same
        // cooperative cancel signal that `RunHandle::cancel` uses is fired, so a
        // hung node without its own `timeout_s` is reclaimed even after every
        // waiter has detached (IF-047). The timer is aborted once the run ends.
        let deadline_timer = self.run_deadline.map(|deadline| {
            let timer_cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(deadline).await;
                request_cancellation(&timer_cancel);
            })
        });

        tokio::spawn(async move {
            // Keep the watch channel open when every external waiter detaches.
            let _cancel_owner = cancel_owner;
            let (lease_stop, lease_stopped) = oneshot::channel();
            let mut heartbeat = tokio::spawn(super::lease::heartbeat_with_timing(
                self.store.clone(),
                self.run_id.clone(),
                self.lease_owner.clone(),
                lease_cancel,
                lease_stopped,
                self.heartbeat_refresh,
                self.heartbeat_timeout,
            ));
            let result = self.supervise(cancel_rx).await;
            let _ = lease_stop.send(());
            if tokio::time::timeout(super::lease::STATE_OPERATION_TIMEOUT, &mut heartbeat)
                .await
                .is_err()
            {
                error!(run_id = %self.run_id, "Workflow lease heartbeat did not stop promptly");
                heartbeat.abort();
                let _ = heartbeat.await;
            }
            if let Some(timer) = deadline_timer {
                timer.abort();
            }
            let _ = completion_tx.send(result);
        });

        RunHandle {
            run_id,
            cancel,
            completion,
        }
    }

    async fn supervise(&self, mut cancel: watch::Receiver<ExecutionSignal>) -> Result<()> {
        // Run execution against a cloned receiver so this supervisor retains
        // an independent cancellation waiter. Dropping the complete `run`
        // future is essential: a storage/node future that ignores cooperative
        // cancellation must not strand the durable handle or admission permit.
        let mut execution_cancel = cancel.clone();
        let workers = CooperativeWorkerSet::new();
        let execution = with_run_worker_set(workers.clone(), self.run(&mut execution_cancel));
        let mut execution = Box::pin(AssertUnwindSafe(execution).catch_unwind());
        let outcome = tokio::select! {
            biased;
            outcome = stop_requested(&mut cancel) => outcome,
            execution = &mut execution => {
                let execution_outcome = match execution {
                    Ok(outcome) => outcome,
                    Err(payload) => {
                    let message = self
                        .execution_overlay
                        .redact_text(&panic_message(payload.as_ref()));
                    error!(run_id = %self.run_id, panic = %message, "Workflow coordinator caught a panic");
                    ExecutionOutcome::Infrastructure(anyhow::anyhow!(
                        "workflow execution panicked: {message}"
                    ))
                    }
                };
                // Re-read the authoritative typed signal after `run` settles.
                // This closes the select window where cancellation and lease
                // loss become ready together; infrastructure always wins.
                cancel.borrow().outcome().unwrap_or(execution_outcome)
            },
        };
        drop(execution);
        // Cancellation drops the async execution future immediately, but a
        // cooperative blocking worker may still be removing temporary output.
        // Do not complete the RunHandle (and release API admission) before it
        // has physically stopped.
        workers.wait_until_idle().await;

        match tokio::time::timeout(self.finalization_timeout, self.finalize(outcome)).await {
            Ok(result) => result,
            Err(_) => {
                error!(
                    run_id = %self.run_id,
                    timeout_ms = self.finalization_timeout.as_millis(),
                    "Workflow finalization timed out; releasing local ownership for lease reconciliation"
                );
                Err(anyhow::anyhow!(
                    "Run '{}' finalization timed out after {}ms",
                    self.run_id,
                    self.finalization_timeout.as_millis()
                ))
            }
        }
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[cfg(test)]
mod tests;
