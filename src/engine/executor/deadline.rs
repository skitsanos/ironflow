use std::future::Future;
use std::time::Duration;

use tokio::time::Instant;

use crate::util::duration::positive_duration;
use crate::util::execution::{CooperativeWorkerSet, with_attempt_worker_set};

/// One deadline shared by every execution attempt and retry backoff for a step.
#[derive(Debug, Clone, Copy)]
pub(super) struct StepDeadline {
    expires_at: Option<Instant>,
    timeout_s: Option<f64>,
}

impl StepDeadline {
    pub(super) fn new(timeout_s: Option<f64>) -> anyhow::Result<Self> {
        let Some(timeout_s) = timeout_s else {
            return Ok(Self {
                expires_at: None,
                timeout_s: None,
            });
        };

        let duration = positive_duration(timeout_s, "step timeout")?;
        let expires_at = Instant::now()
            .checked_add(duration)
            .ok_or_else(|| anyhow::anyhow!("step timeout is too large to schedule"))?;

        Ok(Self {
            expires_at: Some(expires_at),
            timeout_s: Some(timeout_s),
        })
    }

    pub(super) fn instant(self) -> Option<Instant> {
        self.expires_at
    }

    pub(super) async fn run<F>(self, future: F) -> Result<F::Output, ()>
    where
        F: Future,
    {
        match self.expires_at {
            Some(expires_at) => match tokio::time::timeout_at(expires_at, future).await {
                // `timeout_at` cannot interrupt a future that monopolizes its
                // poll. Check again after it returns so such work cannot be
                // accepted merely because it yielded `Ready` late.
                Ok(output) if Instant::now() < expires_at => Ok(output),
                Ok(_) | Err(_) => Err(()),
            },
            None => Ok(future.await),
        }
    }

    /// Apply the deadline and retain task capacity until any timed-out
    /// cooperative blocking workers have physically stopped.
    pub(super) async fn run_tracked<F>(self, future: F) -> Result<F::Output, ()>
    where
        F: Future,
    {
        let workers = CooperativeWorkerSet::new();
        let result = self
            .run(with_attempt_worker_set(workers.clone(), future))
            .await;
        workers.wait_until_idle().await;
        result
    }

    pub(super) async fn sleep(self, duration: Duration) -> Result<(), ()> {
        self.run(tokio::time::sleep(duration)).await
    }

    pub(super) fn error_message(self, step_name: &str) -> String {
        format!(
            "Task '{}' timed out after {}s total",
            step_name,
            self.timeout_s
                .expect("a deadline error requires a configured timeout")
        )
    }
}
