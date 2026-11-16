//! Event publication scoped to one run incarnation (IF-184, IF-214).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tracing::{warn, warn_span};

use crate::engine::events::RunEvent;
use crate::engine::types::Context;
use crate::metrics::{EventAdmissionOutcome, Metrics};
use crate::storage::event_store::EventStore;
use crate::storage::{StateStore, StorageResult};

#[cfg(test)]
mod tests;

/// Admission is the run's first write to the event store. It is retried so
/// one transient failure does not mute a whole run, and bounded so a store
/// that is down delays the start of a run by a few seconds at most: one
/// attempt per entry, after the given pause.
const ADMISSION_DELAYS: [Duration; 3] = [
    Duration::ZERO,
    Duration::from_millis(50),
    Duration::from_millis(200),
];
const ADMISSION_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(1);
/// Bound the one state write that records a failed admission on the run.
const WARNING_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// Context key that records a failed admission on the run, so the cause of
/// an empty event stream is visible through `GET /runs/{id}`.
pub(crate) const EVENT_STREAM_WARNING_KEY: &str = "_event_stream_warning";

/// Publishes every event of one run under that run's incarnation, so a
/// deletion fence left by an earlier run with the same ID does not mute it
/// and a fence installed for this one keeps rejecting it.
struct IncarnationEvents {
    inner: Arc<dyn EventStore>,
    incarnation: String,
}

/// The store a run publishes through, and what to record on the run when
/// it could not be admitted.
pub(super) struct Admission {
    pub(super) events: Arc<dyn EventStore>,
    pub(super) warning: Option<String>,
}

/// Admit `incarnation` as the publisher for `run_id` and return the store
/// its coordinator publishes through.
///
/// Events are best-effort: a run whose state is already durable is not
/// failed because its stream could not be opened. After the bounded retries
/// the failure is counted and returned as a warning for the run record
/// instead of being left in the log only (IF-214). The warning names the
/// failure category, never the error text.
pub(super) async fn begin(
    events: &Arc<dyn EventStore>,
    run_id: &str,
    incarnation: &str,
    metrics: Option<&Arc<Metrics>>,
) -> Admission {
    let admitted = admit(events.as_ref(), run_id, incarnation).await;
    if let Some(metrics) = metrics {
        metrics.event_admission(match admitted {
            Ok(false) => EventAdmissionOutcome::Admitted,
            Ok(true) => EventAdmissionOutcome::AdmittedAfterRetry,
            Err(_) => EventAdmissionOutcome::Failed,
        });
    }
    let warning = admitted.err().map(|cause| {
        format!(
            "The event stream could not be opened after {} attempts (last attempt: {cause}). \
             The run executes normally; its events and SSE stream may be empty.",
            ADMISSION_DELAYS.len()
        )
    });
    Admission {
        events: Arc::new(IncarnationEvents {
            inner: events.clone(),
            incarnation: incarnation.to_string(),
        }),
        warning,
    }
}

/// `Ok(retried)` once the store admitted the incarnation, or the category
/// of the last failure when every attempt failed.
async fn admit(
    events: &dyn EventStore,
    run_id: &str,
    incarnation: &str,
) -> Result<bool, &'static str> {
    let mut cause = "timed out";
    for (attempt, delay) in ADMISSION_DELAYS.iter().enumerate() {
        if !delay.is_zero() {
            tokio::time::sleep(*delay).await;
        }
        let admitted = tokio::time::timeout(
            ADMISSION_ATTEMPT_TIMEOUT,
            events.begin_incarnation(run_id, incarnation),
        )
        .await;
        let _span = warn_span!("workflow_event_incarnation").entered();
        match admitted {
            Ok(Ok(())) => return Ok(attempt > 0),
            Ok(Err(error)) => {
                cause = "store error";
                warn!(%run_id, attempt, error = %error, "Failed to open the workflow event stream");
            }
            Err(_) => {
                cause = "timed out";
                warn!(
                    %run_id,
                    attempt,
                    timeout_ms = ADMISSION_ATTEMPT_TIMEOUT.as_millis(),
                    "Opening the workflow event stream timed out"
                );
            }
        }
    }
    Err(cause)
}

/// Record a failed admission on the run: in the context the coordinator
/// starts from, so it is part of the final record, and in the store now, so
/// it is visible while the run executes. The store write is best-effort.
pub(super) async fn record_warning(
    store: &dyn StateStore,
    run_id: &str,
    owner: &str,
    ctx: &mut Context,
    warning: String,
) {
    let warning = serde_json::Value::String(warning);
    let update = Context::from([(EVENT_STREAM_WARNING_KEY.to_string(), warning.clone())]);
    ctx.insert(EVENT_STREAM_WARNING_KEY.to_string(), warning);
    let written = tokio::time::timeout(
        WARNING_WRITE_TIMEOUT,
        store.update_ctx_owned(run_id, &update, owner),
    )
    .await;
    let failure = match written {
        Ok(Ok(true)) => return,
        Ok(Ok(false)) => "run ownership was lost".to_string(),
        Ok(Err(error)) => error.to_string(),
        Err(_) => "the state write timed out".to_string(),
    };
    let _span = warn_span!("workflow_event_incarnation").entered();
    warn!(%run_id, %failure, "Failed to record the event warning on the run");
}

#[async_trait]
impl EventStore for IncarnationEvents {
    async fn healthcheck(&self) -> StorageResult<()> {
        self.inner.healthcheck().await
    }

    async fn publish(&self, event: RunEvent) -> StorageResult<()> {
        self.inner.publish_as(event, &self.incarnation).await
    }

    async fn publish_as(&self, event: RunEvent, incarnation: &str) -> StorageResult<()> {
        self.inner.publish_as(event, incarnation).await
    }

    async fn begin_incarnation(&self, run_id: &str, incarnation: &str) -> StorageResult<()> {
        self.inner.begin_incarnation(run_id, incarnation).await
    }

    async fn admitted_incarnation(&self, run_id: &str) -> StorageResult<Option<String>> {
        self.inner.admitted_incarnation(run_id).await
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<usize> {
        self.inner.delete_run(run_id).await
    }

    async fn delete_incarnation(&self, run_id: &str, incarnation: &str) -> StorageResult<usize> {
        self.inner.delete_incarnation(run_id, incarnation).await
    }

    async fn delete_orphaned_incarnation(
        &self,
        run_id: &str,
        seen: Option<&str>,
    ) -> StorageResult<usize> {
        self.inner.delete_orphaned_incarnation(run_id, seen).await
    }

    async fn delete_orphaned_run(&self, run_id: &str) -> StorageResult<usize> {
        self.inner.delete_orphaned_run(run_id).await
    }

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> StorageResult<Vec<RunEvent>> {
        self.inner.list_since(run_id, after, limit).await
    }
}
