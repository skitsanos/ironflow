//! Event publication scoped to one run incarnation (IF-184).

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{warn, warn_span};

use crate::engine::events::RunEvent;
use crate::storage::StorageResult;
use crate::storage::event_store::EventStore;

/// Bound the one call that admits a new incarnation past a deletion fence.
const BEGIN_INCARNATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Publishes every event of one run under that run's incarnation, so a
/// deletion fence left by an earlier run with the same ID does not mute it
/// and a fence installed for this one keeps rejecting it.
struct IncarnationEvents {
    inner: Arc<dyn EventStore>,
    incarnation: String,
}

/// Admit `incarnation` as the publisher for `run_id` and return the store
/// its coordinator publishes through.
///
/// Events are best-effort: a failure here is logged and leaves any existing
/// fence in place, which rejects this incarnation's events rather than
/// failing a run whose state is already durable.
pub(super) async fn begin(
    events: &Arc<dyn EventStore>,
    run_id: &str,
    incarnation: &str,
) -> Arc<dyn EventStore> {
    let admitted = tokio::time::timeout(
        BEGIN_INCARNATION_TIMEOUT,
        events.begin_incarnation(run_id, incarnation),
    )
    .await;
    match admitted {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _span = warn_span!("workflow_event_incarnation").entered();
            warn!(%run_id, error = %error, "Failed to open the workflow event stream");
        }
        Err(_) => {
            let _span = warn_span!("workflow_event_incarnation").entered();
            warn!(
                %run_id,
                timeout_ms = BEGIN_INCARNATION_TIMEOUT.as_millis(),
                "Opening the workflow event stream timed out"
            );
        }
    }
    Arc::new(IncarnationEvents {
        inner: events.clone(),
        incarnation: incarnation.to_string(),
    })
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

    async fn delete_run(&self, run_id: &str) -> StorageResult<usize> {
        self.inner.delete_run(run_id).await
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
