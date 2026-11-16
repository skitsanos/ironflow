//! Event stores that misbehave on purpose, to check what the complete-stream
//! contract tolerates (IF-220 aa).
//!
//! The engine publishes every event with a 500 ms bound and drops the
//! publication when the bound passes: events are best-effort. A test of
//! something else must survive that on a loaded host, and must still fail
//! when an event is lost for any other reason.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ironflow::engine::RunEvent;
use ironflow::storage::StorageResult;
use ironflow::storage::event_store::EventStore;

/// What the store does to the first event it is asked to publish.
#[derive(Clone, Copy)]
pub enum Fault {
    /// Take longer than the engine's publish bound, so the engine abandons
    /// the publication, as it does on a saturated host.
    OutlastTheBound,
    /// Report success without storing the event: a loss no bound explains.
    DropSilently,
}

pub struct FaultyOnce {
    inner: Arc<dyn EventStore>,
    fault: Fault,
    pending: AtomicBool,
}

impl FaultyOnce {
    pub fn new(inner: Arc<dyn EventStore>, fault: Fault) -> Arc<Self> {
        Arc::new(Self {
            inner,
            fault,
            pending: AtomicBool::new(true),
        })
    }

    /// Whether this publication is the one the fault applies to, and if so
    /// whether it must be skipped.
    async fn skip(&self) -> bool {
        if !self.pending.swap(false, Ordering::SeqCst) {
            return false;
        }
        match self.fault {
            Fault::OutlastTheBound => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                false
            }
            Fault::DropSilently => true,
        }
    }
}

#[async_trait::async_trait]
impl EventStore for FaultyOnce {
    async fn publish(&self, event: RunEvent) -> StorageResult<()> {
        if self.skip().await {
            return Ok(());
        }
        self.inner.publish(event).await
    }

    async fn publish_as(&self, event: RunEvent, incarnation: &str) -> StorageResult<()> {
        if self.skip().await {
            return Ok(());
        }
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

    async fn delete_orphaned_run(&self, run_id: &str) -> StorageResult<usize> {
        self.inner.delete_orphaned_run(run_id).await
    }

    async fn delete_orphaned_incarnation(
        &self,
        run_id: &str,
        seen: Option<&str>,
    ) -> StorageResult<usize> {
        self.inner.delete_orphaned_incarnation(run_id, seen).await
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
