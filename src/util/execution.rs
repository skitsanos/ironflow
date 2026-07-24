//! Cooperative execution deadlines for node work.
//!
//! The executor scopes a task-local deadline around node execution. Synchronous
//! workers snapshot that deadline and receive a cancellation flag which is set
//! when their async waiter is dropped (for example, by `timeout_at`).

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use tokio::time::Instant;

tokio::task_local! {
    static EXECUTION_DEADLINE: Option<Instant>;
}

/// Run `future` with the total step deadline visible to nested node work.
pub async fn with_execution_deadline<F>(deadline: Option<Instant>, future: F) -> F::Output
where
    F: Future,
{
    EXECUTION_DEADLINE.scope(deadline, future).await
}

/// Return the deadline scoped by the executor, if this task has one.
pub fn current_execution_deadline() -> Option<Instant> {
    EXECUTION_DEADLINE
        .try_with(|deadline| *deadline)
        .ok()
        .flatten()
}

/// Deadline and cooperative cancellation state shared with synchronous work.
#[derive(Clone, Debug)]
pub struct ExecutionControl {
    cancelled: Arc<AtomicBool>,
    deadline: Option<std::time::Instant>,
}

impl ExecutionControl {
    fn for_current_task() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: current_execution_deadline().map(Instant::into_std),
        }
    }

    /// Whether the async owner stopped waiting for this work.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Whether the executor's total step deadline has elapsed.
    pub fn deadline_exceeded(&self) -> bool {
        self.deadline
            .is_some_and(|deadline| std::time::Instant::now() >= deadline)
    }

    /// Fail synchronous work at a cooperative checkpoint.
    pub fn checkpoint(&self) -> Result<()> {
        if self.is_cancelled() {
            anyhow::bail!("step execution cancelled");
        }
        if self.deadline_exceeded() {
            anyhow::bail!("step deadline exceeded");
        }
        Ok(())
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

struct CancelOnDrop {
    control: ExecutionControl,
    armed: bool,
}

impl CancelOnDrop {
    fn new(control: ExecutionControl) -> Self {
        Self {
            control,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if self.armed {
            self.control.cancel();
        }
    }
}

/// Run synchronous node work on Tokio's blocking pool.
///
/// Dropping this future signals cooperative cancellation to `operation`.
pub async fn run_blocking_step<T, F>(operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(ExecutionControl) -> Result<T> + Send + 'static,
{
    let control = ExecutionControl::for_current_task();
    control.checkpoint()?;

    let worker_control = control.clone();
    let handle = tokio::task::spawn_blocking(move || operation(worker_control));
    let mut cancel_on_drop = CancelOnDrop::new(control);
    let outcome = handle.await;
    cancel_on_drop.disarm();

    outcome.map_err(|error| anyhow::anyhow!("blocking step worker failed: {error}"))?
}
