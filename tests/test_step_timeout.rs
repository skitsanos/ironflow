use std::collections::HashMap;
use std::future::pending;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use async_trait::async_trait;
use ironflow::engine::RunEventType;
use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{
    Context, FlowDefinition, NodeOutput, RetryConfig, RunStatus, StepDefinition, TaskStatus,
};
use ironflow::nodes::{Node, NodeRegistry};
use ironflow::storage::StateStore;
use ironflow::storage::event_store::{EventStore, MemoryEventStore};
use ironflow::storage::null_store::NullStateStore;

fn step(timeout_s: f64, max_retries: u32, backoff_s: f64) -> StepDefinition {
    StepDefinition {
        name: "work".to_string(),
        node_type: "test_timeout".to_string(),
        config: serde_json::json!({}),
        dependencies: vec![],
        retry: RetryConfig {
            max_retries,
            backoff_s,
        },
        timeout_s: Some(timeout_s),
        route: None,
        on_error: None,
    }
}

fn engine(node: Arc<dyn Node>) -> (WorkflowEngine, Arc<NullStateStore>, Arc<MemoryEventStore>) {
    let mut registry = NodeRegistry::new();
    registry.register(node);
    let store = Arc::new(NullStateStore::new());
    let events = Arc::new(MemoryEventStore::new());
    let engine =
        WorkflowEngine::new_with_events(Arc::new(registry), store.clone(), events.clone(), None);
    (engine, store, events)
}

struct AlwaysFailNode {
    attempts: Arc<AtomicUsize>,
}

#[async_trait]
impl Node for AlwaysFailNode {
    fn node_type(&self) -> &str {
        "test_timeout"
    }

    fn description(&self) -> &str {
        "Fail immediately for total-timeout tests"
    }

    async fn execute(&self, _config: &serde_json::Value, _ctx: &Context) -> Result<NodeOutput> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        bail!("injected attempt failure")
    }
}

#[tokio::test]
async fn total_deadline_includes_retry_backoff() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let (engine, store, events) = engine(Arc::new(AlwaysFailNode {
        attempts: attempts.clone(),
    }));
    let flow = FlowDefinition {
        name: "backoff_deadline".to_string(),
        steps: vec![step(0.05, 3, 0.2)],
    };

    let started = Instant::now();
    let run_id = engine.execute(&flow, Context::new()).await.unwrap();
    let elapsed = started.elapsed();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert!(elapsed < Duration::from_millis(500), "elapsed: {elapsed:?}");
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.tasks["work"].attempt, 1);
    assert_eq!(
        info.tasks["work"].error.as_deref(),
        Some("Task 'work' timed out after 0.05s total")
    );

    let retry_events = events
        .list_since(&run_id, None, 100)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == RunEventType::TaskRetrying)
        .count();
    assert_eq!(retry_events, 0, "no retry was actually started");
}

#[tokio::test]
async fn explicit_cancellation_during_backoff_is_persisted_deterministically() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let (engine, store, _) = engine(Arc::new(AlwaysFailNode {
        attempts: attempts.clone(),
    }));
    let flow = FlowDefinition {
        name: "cancel_backoff".to_string(),
        steps: vec![step(5.0, 3, 5.0)],
    };

    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let info = store.get_run_info(&run_id).await.unwrap();
            if info
                .tasks
                .get("work")
                .is_some_and(|task| task.status == TaskStatus::Running && task.error.is_some())
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("step did not enter retry backoff");

    handle.cancel().await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(info.status, RunStatus::Cancelled);
    assert_eq!(info.tasks["work"].status, TaskStatus::Cancelled);
    assert_eq!(
        info.tasks["work"].error.as_deref(),
        Some("workflow execution was cancelled")
    );
}

struct SlowFailureNode {
    attempts: Arc<AtomicUsize>,
}

#[async_trait]
impl Node for SlowFailureNode {
    fn node_type(&self) -> &str {
        "test_timeout"
    }

    fn description(&self) -> &str {
        "Consume part of a shared timeout on every attempt"
    }

    async fn execute(&self, _config: &serde_json::Value, _ctx: &Context) -> Result<NodeOutput> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(80)).await;
        bail!("injected slow failure")
    }
}

#[tokio::test]
async fn retries_receive_only_the_remaining_timeout_budget() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let (engine, store, _) = engine(Arc::new(SlowFailureNode {
        attempts: attempts.clone(),
    }));
    let flow = FlowDefinition {
        name: "attempt_deadline".to_string(),
        steps: vec![step(0.2, 4, 0.0)],
    };

    let started = Instant::now();
    let run_id = engine.execute(&flow, Context::new()).await.unwrap();
    let elapsed = started.elapsed();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert!(elapsed < Duration::from_millis(600), "elapsed: {elapsed:?}");
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert_eq!(info.tasks["work"].attempt, 3);
    assert_eq!(info.tasks["work"].status, TaskStatus::Failed);
    assert_eq!(
        info.tasks["work"].error.as_deref(),
        Some("Task 'work' timed out after 0.2s total")
    );
}

struct FailOnceNode {
    attempts: Arc<AtomicUsize>,
}

#[async_trait]
impl Node for FailOnceNode {
    fn node_type(&self) -> &str {
        "test_timeout"
    }

    fn description(&self) -> &str {
        "Succeed on the second attempt"
    }

    async fn execute(&self, _config: &serde_json::Value, _ctx: &Context) -> Result<NodeOutput> {
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            bail!("injected first-attempt failure");
        }
        Ok(HashMap::from([(
            "recovered".to_string(),
            serde_json::json!(true),
        )]))
    }
}

#[tokio::test]
async fn retry_can_succeed_within_the_total_budget() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let (engine, store, events) = engine(Arc::new(FailOnceNode {
        attempts: attempts.clone(),
    }));
    let flow = FlowDefinition {
        name: "retry_success".to_string(),
        steps: vec![step(0.5, 2, 0.01)],
    };

    let run_id = engine.execute(&flow, Context::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks["work"].attempt, 2);
    assert_eq!(info.tasks["work"].status, TaskStatus::Success);
    assert_eq!(info.ctx["recovered"], serde_json::json!(true));

    let retry_events = events
        .list_since(&run_id, None, 100)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == RunEventType::TaskRetrying)
        .count();
    assert_eq!(retry_events, 1);
}

struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct PendingNode {
    dropped: Arc<AtomicBool>,
}

#[async_trait]
impl Node for PendingNode {
    fn node_type(&self) -> &str {
        "test_timeout"
    }

    fn description(&self) -> &str {
        "Stay pending until the executor drops the attempt"
    }

    async fn execute(&self, _config: &serde_json::Value, _ctx: &Context) -> Result<NodeOutput> {
        let _drop_flag = DropFlag(self.dropped.clone());
        pending().await
    }
}

#[tokio::test]
async fn timeout_drops_async_node_work_and_records_stable_error() {
    let dropped = Arc::new(AtomicBool::new(false));
    let (engine, store, _) = engine(Arc::new(PendingNode {
        dropped: dropped.clone(),
    }));
    let flow = FlowDefinition {
        name: "pending_deadline".to_string(),
        steps: vec![step(0.05, 2, 0.0)],
    };

    let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(info.tasks["work"].attempt, 1);
    assert_eq!(
        info.tasks["work"].error.as_deref(),
        Some("Task 'work' timed out after 0.05s total")
    );
}
