use std::sync::Arc;

use async_trait::async_trait;

use super::*;
use crate::engine::types::FlowDefinition;
use crate::nodes::NodeRegistry;
use crate::storage::event_store::EventStore;
use crate::storage::null_store::NullStateStore;
use crate::storage::{StateStore, StorageResult};

struct HangingEventStore;

#[async_trait]
impl EventStore for HangingEventStore {
    async fn publish(&self, _: RunEvent) -> StorageResult<()> {
        std::future::pending().await
    }

    async fn delete_run(&self, _: &str) -> StorageResult<usize> {
        unreachable!()
    }

    async fn list_since(&self, _: &str, _: Option<&str>, _: usize) -> StorageResult<Vec<RunEvent>> {
        unreachable!()
    }
}

#[tokio::test]
async fn hanging_event_backend_does_not_hold_the_run_permit() {
    let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new_with_events(
        Arc::new(NodeRegistry::with_builtins()),
        store,
        Arc::new(HangingEventStore),
        Some(1),
    );
    let handle = engine
        .start(
            &FlowDefinition {
                name: "hanging-events".to_string(),
                steps: Vec::new(),
            },
            Context::new(),
        )
        .await
        .unwrap();
    let admission = Arc::new(tokio::sync::Semaphore::new(1));
    let permit = admission.clone().acquire_owned().await.unwrap();
    let waiter = tokio::spawn(async move {
        let _permit = permit;
        handle.wait().await
    });

    tokio::time::timeout(std::time::Duration::from_secs(3), waiter)
        .await
        .expect("a hanging event backend stranded the run handle")
        .unwrap()
        .unwrap();
    let _permit = tokio::time::timeout(std::time::Duration::from_millis(100), admission.acquire())
        .await
        .expect("the completed run kept its admission permit")
        .unwrap();
}
