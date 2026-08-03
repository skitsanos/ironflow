//! Process lifecycle shared by HTTP admission, schedules, and shutdown.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;

use crate::engine::{RunCancellation, RunHandle};

const CANCEL_SETTLE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Default)]
pub struct ServiceLifecycle {
    inner: Arc<LifecycleInner>,
}

#[derive(Default)]
struct LifecycleInner {
    draining: AtomicBool,
    active: Mutex<HashMap<String, RunCancellation>>,
    draining_changed: Notify,
    active_changed: Notify,
}

impl ServiceLifecycle {
    pub(crate) fn is_ready(&self) -> bool {
        !self.inner.draining.load(Ordering::Acquire)
    }

    /// Atomically close admission and wake listener/scheduler shutdown waiters.
    pub(crate) fn begin_draining(&self) -> bool {
        let changed = !self.inner.draining.swap(true, Ordering::AcqRel);
        if changed {
            tracing::info!(active_runs = self.active_runs(), "service is draining");
            self.inner.draining_changed.notify_waiters();
        }
        changed
    }

    pub(crate) async fn wait_for_draining(&self) {
        loop {
            let changed = self.inner.draining_changed.notified();
            if !self.is_ready() {
                return;
            }
            changed.await;
        }
    }

    pub(crate) fn active_runs(&self) -> usize {
        self.inner
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    pub(crate) fn track(&self, handle: &RunHandle) -> ActiveRun {
        let run_id = handle.id().to_string();
        let cancellation = handle.cancellation();
        self.inner
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(run_id.clone(), cancellation.clone());

        // Close the registration race where draining begins after a caller's
        // admission check but before its durable run is registered.
        if !self.is_ready() {
            cancellation.request();
        }

        ActiveRun {
            run_id: Some(run_id),
            lifecycle: self.clone(),
        }
    }

    /// Wait for accepted work, then cooperatively cancel what exceeded the
    /// operator's grace period. The second wait is bounded so shutdown cannot
    /// hang indefinitely on a defective coordinator.
    pub(crate) async fn drain(&self, grace: Duration) -> bool {
        if self.wait_until_idle(grace).await {
            return true;
        }

        let cancellations = self
            .inner
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        tracing::warn!(
            active_runs = cancellations.len(),
            grace_ms = grace.as_millis(),
            "shutdown grace expired; cancelling active runs"
        );
        for cancellation in cancellations {
            cancellation.request();
        }

        let settled = self.wait_until_idle(CANCEL_SETTLE_TIMEOUT).await;
        if !settled {
            tracing::error!(
                active_runs = self.active_runs(),
                timeout_ms = CANCEL_SETTLE_TIMEOUT.as_millis(),
                "active runs did not settle before forced process shutdown"
            );
        }
        settled
    }

    async fn wait_until_idle(&self, timeout: Duration) -> bool {
        tokio::time::timeout(timeout, async {
            loop {
                let changed = self.inner.active_changed.notified();
                if self.active_runs() == 0 {
                    return;
                }
                changed.await;
            }
        })
        .await
        .is_ok()
    }

    fn remove(&self, run_id: &str) {
        self.inner
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(run_id);
        self.inner.active_changed.notify_waiters();
    }
}

pub(crate) struct ActiveRun {
    run_id: Option<String>,
    lifecycle: ServiceLifecycle,
}

impl Drop for ActiveRun {
    fn drop(&mut self) {
        if let Some(run_id) = self.run_id.take() {
            self.lifecycle.remove(&run_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn draining_wakes_waiters_and_closes_readiness() {
        let lifecycle = ServiceLifecycle::default();
        let waiter = tokio::spawn({
            let lifecycle = lifecycle.clone();
            async move { lifecycle.wait_for_draining().await }
        });
        assert!(lifecycle.is_ready());
        assert!(lifecycle.begin_draining());
        assert!(!lifecycle.begin_draining());
        waiter.await.unwrap();
        assert!(!lifecycle.is_ready());
    }

    #[tokio::test]
    async fn expired_grace_cancels_and_settles_a_tracked_run() {
        use crate::engine::types::{Context, FlowDefinition, RunStatus, StepDefinition};
        use crate::engine::{RetryConfig, WorkflowEngine};
        use crate::nodes::NodeRegistry;
        use crate::storage::StateStore;

        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(crate::storage::json_store::JsonStateStore::new(
            directory.path(),
        ));
        let engine = WorkflowEngine::new(
            Arc::new(NodeRegistry::with_builtins()),
            store.clone(),
            Some(1),
        );
        let flow = FlowDefinition {
            name: "drain".to_string(),
            steps: vec![StepDefinition {
                name: "hold".to_string(),
                node_type: "delay".to_string(),
                config: serde_json::json!({"seconds": 30}),
                dependencies: Vec::new(),
                retry: RetryConfig::default(),
                timeout_s: None,
                route: None,
                on_error: None,
            }],
        };
        let handle = engine.start(&flow, Context::new()).await.unwrap();
        let run_id = handle.id().to_string();
        let lifecycle = ServiceLifecycle::default();
        let active = lifecycle.track(&handle);
        let waiter = tokio::spawn(async move {
            let _active = active;
            handle.wait().await
        });

        lifecycle.begin_draining();
        assert!(lifecycle.drain(Duration::ZERO).await);
        waiter.await.unwrap().unwrap();
        assert_eq!(lifecycle.active_runs(), 0);
        assert_eq!(
            store.get_run_info(&run_id).await.unwrap().status,
            RunStatus::Cancelled
        );
    }
}
