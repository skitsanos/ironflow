use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use futures_util::FutureExt as _;
use tokio::sync::{RwLock, oneshot, watch};
use tracing::error;

use crate::engine::recovery::ExecutionPlan;
use crate::engine::types::{Context, FlowDefinition, RunStatus};
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

use super::overlay::ExecutionOverlay;

/// Handle for a supervised workflow execution.
///
/// Dropping this value detaches from the run without cancelling it. This keeps
/// HTTP disconnects and cancelled waiters from stranding durable state.
pub struct RunHandle {
    run_id: String,
    cancel: watch::Sender<bool>,
    completion: oneshot::Receiver<Result<()>>,
}

struct CancelRunOnDrop {
    cancel: Option<watch::Sender<bool>>,
}

impl CancelRunOnDrop {
    fn new(cancel: watch::Sender<bool>) -> Self {
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
            let _ = cancel.send(true);
        }
    }
}

impl RunHandle {
    pub fn id(&self) -> &str {
        &self.run_id
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
            let _ = self.cancel.send(true);
        }

        self.completion.await.with_context(|| {
            format!("Run coordinator for '{}' stopped unexpectedly", self.run_id)
        })??;
        Ok(self.run_id)
    }
}

pub(super) enum ExecutionOutcome {
    Completed(RunStatus),
    Cancelled,
    Infrastructure(anyhow::Error),
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
        }
    }

    pub(super) fn spawn(self) -> RunHandle {
        let run_id = self.run_id.clone();
        let (cancel, cancel_rx) = watch::channel(false);
        let cancel_owner = cancel.clone();
        let (completion_tx, completion) = oneshot::channel();

        // Optional run-level deadline. When a run outlives it, the same
        // cooperative cancel signal that `RunHandle::cancel` uses is fired, so a
        // hung node without its own `timeout_s` is reclaimed even after every
        // waiter has detached (IF-047). The timer is aborted once the run ends.
        let deadline_timer = run_deadline().map(|deadline| {
            let timer_cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(deadline).await;
                let _ = timer_cancel.send(true);
            })
        });

        tokio::spawn(async move {
            // Keep the watch channel open when every external waiter detaches.
            let _cancel_owner = cancel_owner;
            let result = self.supervise(cancel_rx).await;
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

    async fn supervise(&self, mut cancel: watch::Receiver<bool>) -> Result<()> {
        let execution = AssertUnwindSafe(self.run(&mut cancel)).catch_unwind().await;
        let outcome = match execution {
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

        self.finalize(outcome).await
    }
}

/// Optional process-wide run-level wall-clock deadline, from
/// `IRONFLOW_MAX_RUN_SECONDS` (unset or `0` = no deadline).
fn run_deadline() -> Option<std::time::Duration> {
    std::env::var("IRONFLOW_MAX_RUN_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(std::time::Duration::from_secs)
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
