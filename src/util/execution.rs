//! Cooperative execution deadlines for node work.
//!
//! The executor scopes a task-local deadline around node execution. Synchronous
//! workers snapshot that deadline and receive a cancellation flag which is set
//! when their async waiter is dropped (for example, by `timeout_at`).

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use tokio::time::Instant;

tokio::task_local! {
    static EXECUTION_DEADLINE: Option<Instant>;
    static RUN_WORKERS: CooperativeWorkerSet;
    static ATTEMPT_WORKERS: CooperativeWorkerSet;
}

/// Physical cooperative workers associated with an execution scope.
///
/// Dropping an async waiter can only signal synchronous work; it cannot join a
/// Tokio blocking task. The executor therefore retains these sets separately
/// and waits for tracked workers before reusing task or run capacity.
#[derive(Clone, Debug, Default)]
pub(crate) struct CooperativeWorkerSet {
    inner: Arc<CooperativeWorkerState>,
}

#[derive(Debug, Default)]
struct CooperativeWorkerState {
    active: AtomicUsize,
    idle: tokio::sync::Notify,
}

impl CooperativeWorkerSet {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn register(&self) -> CooperativeWorkerGuard {
        self.inner.active.fetch_add(1, Ordering::AcqRel);
        CooperativeWorkerGuard { set: self.clone() }
    }

    pub(crate) async fn wait_until_idle(&self) {
        loop {
            let idle = self.inner.idle.notified();
            if self.inner.active.load(Ordering::Acquire) == 0 {
                return;
            }
            idle.await;
        }
    }
}

struct CooperativeWorkerGuard {
    set: CooperativeWorkerSet,
}

impl Drop for CooperativeWorkerGuard {
    fn drop(&mut self) {
        if self.set.inner.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.set.inner.idle.notify_waiters();
        }
    }
}

struct CooperativeWorkerGuards {
    _run: Option<CooperativeWorkerGuard>,
    _attempt: Option<CooperativeWorkerGuard>,
}

impl CooperativeWorkerGuards {
    fn for_current_task() -> Self {
        Self {
            _run: RUN_WORKERS.try_with(CooperativeWorkerSet::register).ok(),
            _attempt: ATTEMPT_WORKERS
                .try_with(CooperativeWorkerSet::register)
                .ok(),
        }
    }
}

pub(crate) async fn with_run_worker_set<F>(workers: CooperativeWorkerSet, future: F) -> F::Output
where
    F: Future,
{
    RUN_WORKERS.scope(workers, future).await
}

pub(crate) async fn with_attempt_worker_set<F>(
    workers: CooperativeWorkerSet,
    future: F,
) -> F::Output
where
    F: Future,
{
    ATTEMPT_WORKERS.scope(workers, future).await
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
        tracing::trace!(
            target: "ironflow::execution::cooperative_worker",
            "cooperative blocking worker cancellation requested"
        );
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

/// Run cooperative blocking work whose physical lifetime must be retained by
/// the surrounding task and run admission scopes.
///
/// This is intentionally separate from [`run_blocking_step`]: only operations
/// with bounded, well-placed cancellation checkpoints are safe to retain
/// during executor shutdown.
pub(crate) async fn run_tracked_blocking_step<T, F>(operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(ExecutionControl) -> Result<T> + Send + 'static,
{
    let control = ExecutionControl::for_current_task();
    control.checkpoint()?;

    let worker_control = control.clone();
    let guards = CooperativeWorkerGuards::for_current_task();
    let handle = tokio::task::spawn_blocking(move || {
        let _guards = guards;
        operation(worker_control)
    });
    let mut cancel_on_drop = CancelOnDrop::new(control);
    let outcome = handle.await;
    cancel_on_drop.disarm();

    outcome.map_err(|error| anyhow::anyhow!("blocking step worker failed: {error}"))?
}

#[cfg(test)]
mod tests;
