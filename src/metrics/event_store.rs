use std::sync::Arc;

use async_trait::async_trait;

use super::{Metrics, StorageOperation, StoreKind};
use crate::engine::events::RunEvent;
use crate::storage::StorageResult;
use crate::storage::event_store::EventStore;

pub(crate) fn observe_event_store(
    inner: Arc<dyn EventStore>,
    metrics: Arc<Metrics>,
) -> Arc<dyn EventStore> {
    Arc::new(ObservedEventStore { inner, metrics })
}

struct ObservedEventStore {
    inner: Arc<dyn EventStore>,
    metrics: Arc<Metrics>,
}

impl ObservedEventStore {
    fn observe<T>(
        &self,
        operation: StorageOperation,
        result: StorageResult<T>,
    ) -> StorageResult<T> {
        if let Err(error) = &result {
            self.metrics
                .storage_failure(StoreKind::Event, operation, error.kind());
        }
        result
    }
}

#[async_trait]
impl EventStore for ObservedEventStore {
    async fn healthcheck(&self) -> StorageResult<()> {
        let result = self.inner.healthcheck().await;
        self.observe(StorageOperation::Healthcheck, result)
    }

    async fn publish(&self, event: RunEvent) -> StorageResult<()> {
        let result = self.inner.publish(event).await;
        self.observe(StorageOperation::PublishEvent, result)
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<usize> {
        let result = self.inner.delete_run(run_id).await;
        self.observe(StorageOperation::DeleteRun, result)
    }

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> StorageResult<Vec<RunEvent>> {
        let result = self.inner.list_since(run_id, after, limit).await;
        self.observe(StorageOperation::ListEvents, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingEventStore;

    #[async_trait::async_trait]
    impl EventStore for FailingEventStore {
        async fn publish(&self, _: RunEvent) -> StorageResult<()> {
            unreachable!()
        }

        async fn delete_run(&self, _: &str) -> StorageResult<usize> {
            unreachable!()
        }

        async fn list_since(
            &self,
            _: &str,
            _: Option<&str>,
            _: usize,
        ) -> StorageResult<Vec<RunEvent>> {
            Err(crate::storage::StorageError::backend(
                "list events",
                "if100-secret-diagnostic",
            ))
        }
    }

    #[tokio::test]
    async fn event_store_failures_use_only_bounded_labels() {
        let metrics = Arc::new(Metrics::new());
        let store = observe_event_store(Arc::new(FailingEventStore), metrics.clone());

        store
            .list_since("if100-secret-run", Some("if100-secret-cursor"), 1)
            .await
            .unwrap_err();
        let encoded = metrics.encode().unwrap();

        assert!(encoded.contains(
            "ironflow_storage_failures_total{store=\"event\",operation=\"list_events\",error_kind=\"backend\"} 1"
        ));
        assert!(!encoded.contains("if100-secret"));
    }
}
