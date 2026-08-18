use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::{ActiveLabels, ActiveWorkKind, Metrics, RunOutcome, TaskOutcome};

pub(crate) struct RunObservation {
    metrics: Arc<Metrics>,
    started: Instant,
    finished: AtomicBool,
}

impl RunObservation {
    pub(super) fn new(metrics: Arc<Metrics>) -> Self {
        metrics
            .active_work
            .get_or_create(&ActiveLabels {
                kind: ActiveWorkKind::Run.as_str(),
            })
            .inc();
        Self {
            metrics,
            started: Instant::now(),
            finished: AtomicBool::new(false),
        }
    }

    pub(crate) fn finish_status(&self, status: &crate::engine::types::RunStatus) {
        if let Some(outcome) = RunOutcome::from_status(status) {
            self.finish(outcome);
        }
    }

    pub(crate) fn finish_stalled(&self) {
        self.finish(RunOutcome::Stalled);
    }

    fn finish(&self, outcome: RunOutcome) {
        if self
            .finished
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.metrics
            .active_work
            .get_or_create(&ActiveLabels {
                kind: ActiveWorkKind::Run.as_str(),
            })
            .dec();
        self.metrics.record_run(outcome, self.started.elapsed());
    }
}

impl Drop for RunObservation {
    fn drop(&mut self) {
        self.finish(RunOutcome::Stalled);
    }
}

pub(crate) struct TaskAttemptObservation {
    metrics: Arc<Metrics>,
    started: Instant,
    finished: bool,
}

impl TaskAttemptObservation {
    pub(super) fn new(metrics: Arc<Metrics>) -> Self {
        Self {
            metrics,
            started: Instant::now(),
            finished: false,
        }
    }

    pub(crate) fn finish(mut self, outcome: TaskOutcome) {
        self.metrics
            .record_task_attempt(outcome, self.started.elapsed());
        self.finished = true;
    }
}

impl Drop for TaskAttemptObservation {
    fn drop(&mut self) {
        if !self.finished {
            self.metrics
                .record_task_attempt(TaskOutcome::Aborted, self.started.elapsed());
        }
    }
}

pub(crate) struct ActiveWorkGuard {
    metrics: Arc<Metrics>,
    kind: ActiveWorkKind,
}

impl ActiveWorkGuard {
    pub(super) fn new(metrics: Arc<Metrics>, kind: ActiveWorkKind) -> Self {
        Self { metrics, kind }
    }
}

impl Drop for ActiveWorkGuard {
    fn drop(&mut self) {
        self.metrics
            .active_work
            .get_or_create(&ActiveLabels {
                kind: self.kind.as_str(),
            })
            .dec();
    }
}
