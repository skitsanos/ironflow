use anyhow::{Context as _, Result};
use tokio::sync::{oneshot, watch};

use crate::engine::types::{Context, RunStatus};

use super::signal::{ExecutionSignal, request_cancellation};

/// Invocation-local, redacted child output, never reconstructed from history.
pub(crate) struct ChildRunResult {
    pub(crate) flow_name: String,
    pub(crate) status: RunStatus,
    pub(crate) ctx: Context,
}

/// Handle for a supervised workflow execution.
///
/// Dropping this value detaches from the run without cancelling it. This keeps
/// HTTP disconnects and cancelled waiters from stranding durable state.
pub struct RunHandle {
    run_id: String,
    cancel: watch::Sender<ExecutionSignal>,
    completion: oneshot::Receiver<Result<Option<ChildRunResult>>>,
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

struct CancelRunOnDrop(Option<watch::Sender<ExecutionSignal>>);

impl Drop for CancelRunOnDrop {
    fn drop(&mut self) {
        if let Some(cancel) = self.0.take() {
            request_cancellation(&cancel);
        }
    }
}

impl RunHandle {
    pub(super) fn new(
        run_id: String,
        cancel: watch::Sender<ExecutionSignal>,
        completion: oneshot::Receiver<Result<Option<ChildRunResult>>>,
    ) -> Self {
        Self {
            run_id,
            cancel,
            completion,
        }
    }

    pub fn id(&self) -> &str {
        &self.run_id
    }

    pub(crate) fn cancellation(&self) -> RunCancellation {
        RunCancellation {
            signal: self.cancel.clone(),
        }
    }

    pub async fn wait(self) -> Result<String> {
        self.finish(false).await.map(|(id, _)| id)
    }

    /// Wait for a child result, cancelling it if its parent future is dropped.
    /// Public `wait` deliberately retains detach semantics.
    pub(super) async fn wait_child_cancel_on_drop(self) -> Result<ChildRunResult> {
        let mut guard = CancelRunOnDrop(Some(self.cancel.clone()));
        let result = self.finish(false).await;
        guard.0 = None;
        result?
            .1
            .context("child run did not retain a live completion result")
    }

    pub async fn cancel(self) -> Result<String> {
        self.finish(true).await.map(|(id, _)| id)
    }

    async fn finish(self, cancel: bool) -> Result<(String, Option<ChildRunResult>)> {
        if cancel {
            request_cancellation(&self.cancel);
        }
        let result = self.completion.await.with_context(|| {
            format!("Run coordinator for '{}' stopped unexpectedly", self.run_id)
        })??;
        Ok((self.run_id, result))
    }
}
