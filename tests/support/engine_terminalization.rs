use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use ironflow::engine::types::{
    Context, NodeOutput, RunInfo, RunStatus, RunSummary, TaskState, TaskStatus,
};
use ironflow::nodes::Node;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::storage::{RunListQuery, RunSummaryPage, StateStore, StorageError, StorageResult};
use tokio::sync::{Barrier, Notify, oneshot};

pub struct FaultStore {
    inner: JsonStateStore,
    fail_success_task_write: AtomicBool,
    fail_success_task_name: Mutex<Option<String>>,
    fail_context_write: AtomicBool,
    fail_terminal_status_write: AtomicBool,
    terminal_attempts: AtomicUsize,
}

impl FaultStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            inner: JsonStateStore::new(path),
            fail_success_task_write: AtomicBool::new(false),
            fail_success_task_name: Mutex::new(None),
            fail_context_write: AtomicBool::new(false),
            fail_terminal_status_write: AtomicBool::new(false),
            terminal_attempts: AtomicUsize::new(0),
        }
    }

    pub fn fail_next_success_task_write(&self) {
        self.fail_success_task_write.store(true, Ordering::SeqCst);
    }

    pub fn fail_next_success_task_write_for(&self, task_name: &str) {
        *self.fail_success_task_name.lock().unwrap() = Some(task_name.to_string());
    }

    pub fn fail_next_terminal_status_write(&self) {
        self.fail_terminal_status_write
            .store(true, Ordering::SeqCst);
    }

    pub fn fail_next_context_write(&self) {
        self.fail_context_write.store(true, Ordering::SeqCst);
    }

    pub fn terminal_attempts(&self) -> usize {
        self.terminal_attempts.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl StateStore for FaultStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()> {
        self.inner.init_run(run_id, flow_name, ctx).await
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        if status.is_terminal() {
            self.terminal_attempts.fetch_add(1, Ordering::SeqCst);
            if self
                .fail_terminal_status_write
                .swap(false, Ordering::SeqCst)
            {
                return Err(StorageError::backend(
                    "Injected terminal status failure",
                    "test fault",
                ));
            }
        }
        self.inner.set_run_status(run_id, status).await
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()> {
        let fail_named_task = if task.status == TaskStatus::Success {
            let mut target = self.fail_success_task_name.lock().unwrap();
            if target.as_deref() == Some(task.name.as_str()) {
                target.take();
                true
            } else {
                false
            }
        } else {
            false
        };
        if task.status == TaskStatus::Success
            && (self.fail_success_task_write.swap(false, Ordering::SeqCst) || fail_named_task)
        {
            return Err(StorageError::backend(
                "Injected successful task persistence failure",
                "test fault",
            ));
        }
        self.inner.upsert_task(run_id, task).await
    }

    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context> {
        self.inner.get_ctx(run_id).await
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()> {
        if self.fail_context_write.swap(false, Ordering::SeqCst) {
            return Err(StorageError::backend(
                "Injected final context persistence failure",
                "test fault",
            ));
        }
        self.inner.update_ctx(run_id, ctx).await
    }

    async fn get_run_info(&self, run_id: &str) -> StorageResult<RunInfo> {
        self.inner.get_run_info(run_id).await
    }

    async fn list_runs(&self, status: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        self.inner.list_runs(status).await
    }

    async fn list_run_summaries(
        &self,
        status: Option<RunStatus>,
    ) -> StorageResult<Vec<RunSummary>> {
        self.inner.list_run_summaries(status).await
    }

    async fn list_run_summaries_page(&self, query: &RunListQuery) -> StorageResult<RunSummaryPage> {
        self.inner.list_run_summaries_page(query).await
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<()> {
        self.inner.delete_run(run_id).await
    }
}

pub struct ImmediateNode;

#[async_trait]
impl Node for ImmediateNode {
    fn node_type(&self) -> &str {
        "test_immediate"
    }

    fn description(&self) -> &str {
        "Synthetic node that completes immediately"
    }

    async fn execute(&self, _config: &serde_json::Value, _ctx: &Context) -> Result<NodeOutput> {
        Ok(HashMap::from([(
            "done".to_string(),
            serde_json::json!(true),
        )]))
    }
}

pub struct PanicNode {
    barrier: Arc<Barrier>,
}

impl PanicNode {
    pub fn new(barrier: Arc<Barrier>) -> Self {
        Self { barrier }
    }
}

#[async_trait]
impl Node for PanicNode {
    fn node_type(&self) -> &str {
        "test_panic"
    }

    fn description(&self) -> &str {
        "Synthetic node that panics after synchronization"
    }

    async fn execute(&self, _config: &serde_json::Value, _ctx: &Context) -> Result<NodeOutput> {
        self.barrier.wait().await;
        panic!("intentional node panic")
    }
}

pub struct ControlledNode {
    node_type: &'static str,
    barrier: Option<Arc<Barrier>>,
    release: Arc<Notify>,
    started: Mutex<Option<oneshot::Sender<()>>>,
    dropped: Mutex<Option<oneshot::Sender<()>>>,
}

pub struct ControlledSignals {
    pub release: Arc<Notify>,
    pub started: oneshot::Receiver<()>,
    pub dropped: oneshot::Receiver<()>,
}

impl ControlledNode {
    pub fn new(
        node_type: &'static str,
        barrier: Option<Arc<Barrier>>,
    ) -> (Self, ControlledSignals) {
        let release = Arc::new(Notify::new());
        let (started_tx, started) = oneshot::channel();
        let (dropped_tx, dropped) = oneshot::channel();
        (
            Self {
                node_type,
                barrier,
                release: release.clone(),
                started: Mutex::new(Some(started_tx)),
                dropped: Mutex::new(Some(dropped_tx)),
            },
            ControlledSignals {
                release,
                started,
                dropped,
            },
        )
    }
}

struct DropSignal(Option<oneshot::Sender<()>>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        if let Some(signal) = self.0.take() {
            let _ = signal.send(());
        }
    }
}

#[async_trait]
impl Node for ControlledNode {
    fn node_type(&self) -> &str {
        self.node_type
    }

    fn description(&self) -> &str {
        "Synthetic controllable node"
    }

    async fn execute(&self, _config: &serde_json::Value, _ctx: &Context) -> Result<NodeOutput> {
        let dropped = self.dropped.lock().unwrap().take();
        let _drop_signal = DropSignal(dropped);
        if let Some(started) = self.started.lock().unwrap().take() {
            let _ = started.send(());
        }
        if let Some(barrier) = &self.barrier {
            barrier.wait().await;
        }
        self.release.notified().await;
        Ok(HashMap::from([(
            "controlled_done".to_string(),
            serde_json::json!(true),
        )]))
    }
}
