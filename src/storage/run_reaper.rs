use std::sync::Arc;
use std::time::Duration;

use super::{RUN_LEASE_REFRESH, StateStore};

/// Owns the periodic reconciliation task. Dropping the server-local guard
/// aborts the task instead of detaching it beyond server shutdown.
pub(crate) struct RunLeaseReaper {
    task: tokio::task::JoinHandle<()>,
}

impl Drop for RunLeaseReaper {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(crate) fn spawn_run_lease_reaper(store: Arc<dyn StateStore>) -> RunLeaseReaper {
    RunLeaseReaper {
        task: tokio::spawn(reconcile_loop(store, RUN_LEASE_REFRESH)),
    }
}

async fn reconcile_loop(store: Arc<dyn StateStore>, interval: Duration) {
    let start = tokio::time::Instant::now() + interval;
    let mut ticker = tokio::time::interval_at(start, interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        match tokio::time::timeout(
            interval,
            store.reconcile_expired_run_leases(chrono::Utc::now()),
        )
        .await
        {
            Ok(Ok(0)) => {}
            Ok(Ok(count)) => {
                tracing::info!(count, "reconciled runs with expired ownership leases")
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "periodic run-lease reconciliation failed; will retry")
            }
            Err(_) => tracing::warn!(
                timeout_ms = interval.as_millis(),
                "periodic run-lease reconciliation timed out; will retry"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::engine::types::{Context, RunInfo, RunStatus, TaskState};
    use crate::storage::{RunListQuery, RunSummaryPage, StorageResult};

    struct TransientStore {
        calls: AtomicUsize,
        recovered: tokio::sync::Notify,
    }

    #[async_trait]
    impl StateStore for TransientStore {
        async fn init_run(&self, _: &str, _: &str, _: &Context) -> StorageResult<()> {
            unreachable!()
        }
        async fn set_run_status(&self, _: &str, _: RunStatus) -> StorageResult<()> {
            unreachable!()
        }
        async fn upsert_task(&self, _: &str, _: &TaskState) -> StorageResult<()> {
            unreachable!()
        }
        async fn get_ctx(&self, _: &str) -> StorageResult<Context> {
            unreachable!()
        }
        async fn update_ctx(&self, _: &str, _: &Context) -> StorageResult<()> {
            unreachable!()
        }
        async fn get_run_info(&self, _: &str) -> StorageResult<RunInfo> {
            unreachable!()
        }
        async fn list_runs(&self, _: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
            unreachable!()
        }
        async fn list_run_summaries_page(&self, _: &RunListQuery) -> StorageResult<RunSummaryPage> {
            unreachable!()
        }
        async fn delete_run(&self, _: &str) -> StorageResult<()> {
            unreachable!()
        }
        async fn reconcile_expired_run_leases(
            &self,
            _: chrono::DateTime<chrono::Utc>,
        ) -> StorageResult<usize> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                std::future::pending::<()>().await;
                unreachable!("the first reconciliation is cancelled by its timeout");
            }
            self.recovered.notify_one();
            Ok(1)
        }
    }

    #[tokio::test]
    async fn periodic_reaper_retries_after_a_backend_hang() {
        let store = Arc::new(TransientStore {
            calls: AtomicUsize::new(0),
            recovered: tokio::sync::Notify::new(),
        });
        let task = tokio::spawn(reconcile_loop(store.clone(), Duration::from_millis(2)));
        tokio::time::timeout(Duration::from_secs(1), store.recovered.notified())
            .await
            .expect("periodic reaper stopped after a timed-out backend call");
        assert!(store.calls.load(Ordering::SeqCst) >= 2);
        task.abort();
    }
}
