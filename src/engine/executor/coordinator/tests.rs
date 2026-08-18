use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;

use super::*;
use crate::engine::types::{RetryConfig, RunInfo, RunStatus, StepDefinition, TaskState};
use crate::nodes::NodeRegistry;
use crate::storage::{RunListQuery, RunSummaryPage, StorageResult};

mod lease_loss;

#[derive(Default)]
struct HangingWritesStore {
    task_write_started: AtomicBool,
    context_write_started: AtomicBool,
}

#[async_trait]
impl StateStore for HangingWritesStore {
    async fn init_run(&self, _: &str, _: &str, _: &Context) -> StorageResult<()> {
        unreachable!()
    }

    async fn set_run_status(&self, _: &str, _: RunStatus) -> StorageResult<()> {
        unreachable!()
    }

    async fn upsert_task(&self, _: &str, _: &TaskState) -> StorageResult<()> {
        unreachable!()
    }

    async fn upsert_task_owned(&self, _: &str, _: &TaskState, _: &str) -> StorageResult<bool> {
        self.task_write_started.store(true, Ordering::SeqCst);
        std::future::pending().await
    }

    async fn get_ctx(&self, _: &str) -> StorageResult<Context> {
        unreachable!()
    }

    async fn update_ctx(&self, _: &str, _: &Context) -> StorageResult<()> {
        unreachable!()
    }

    async fn update_ctx_owned(&self, _: &str, _: &Context, _: &str) -> StorageResult<bool> {
        self.context_write_started.store(true, Ordering::SeqCst);
        std::future::pending().await
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
}

#[tokio::test]
async fn deadline_preempts_hanging_task_and_finalizer_writes() {
    let store = Arc::new(HangingWritesStore::default());
    let flow = FlowDefinition {
        name: "hanging-state".to_string(),
        steps: vec![StepDefinition {
            name: "blocked-write".to_string(),
            node_type: "unused".to_string(),
            config: serde_json::Value::Null,
            dependencies: Vec::new(),
            retry: RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        }],
    };
    let execution_plan = ExecutionPlan::build(&flow).unwrap();
    let coordinator = RunCoordinator::new(
        Arc::new(NodeRegistry::with_builtins()),
        store.clone(),
        None,
        1,
        "run".to_string(),
        flow,
        execution_plan,
        Context::new(),
        ExecutionOverlay::default(),
        "owner".to_string(),
        Some(std::time::Duration::from_millis(5)),
        None,
    )
    .with_finalization_timeout(std::time::Duration::from_millis(20));
    let handle = coordinator.spawn();
    let admission = Arc::new(tokio::sync::Semaphore::new(1));
    let permit = admission.clone().acquire_owned().await.unwrap();
    let waiter = tokio::spawn(async move {
        let _permit = permit;
        handle.wait().await
    });

    let result = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
        .await
        .expect("hanging state writes stranded the run handle")
        .unwrap();
    assert!(result.is_err());
    assert!(store.task_write_started.load(Ordering::SeqCst));
    assert!(store.context_write_started.load(Ordering::SeqCst));
    let _permit = tokio::time::timeout(std::time::Duration::from_millis(100), admission.acquire())
        .await
        .expect("the stopped run kept its admission permit")
        .unwrap();
}
