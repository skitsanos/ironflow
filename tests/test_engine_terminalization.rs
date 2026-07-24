//! Regression coverage for supervised run terminalization (IF-004).

#[path = "support/engine_terminalization.rs"]
mod engine_terminalization;

use std::sync::Arc;
use std::time::Duration;

use engine_terminalization::{ControlledNode, FaultStore, ImmediateNode, PanicNode};
use ironflow::engine::RunEventType;
use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{
    Context, FlowDefinition, RetryConfig, RunInfo, RunStatus, StepDefinition, TaskStatus,
};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::event_store::{EventStore, MemoryEventStore};
use tokio::sync::Barrier;

fn step(name: &str, node_type: &str, dependencies: &[&str]) -> StepDefinition {
    StepDefinition {
        name: name.to_string(),
        node_type: node_type.to_string(),
        config: serde_json::json!({}),
        dependencies: dependencies
            .iter()
            .map(|dependency| (*dependency).to_string())
            .collect(),
        retry: RetryConfig::default(),
        timeout_s: None,
        route: None,
        on_error: None,
    }
}

fn assert_fully_terminal(info: &RunInfo) {
    assert!(info.status.is_terminal());
    assert!(info.finished.is_some());
    for task in info.tasks.values() {
        assert!(
            task.status.is_terminal(),
            "task {} was not terminal",
            task.name
        );
        assert!(
            task.finished.is_some(),
            "task {} had no finish time",
            task.name
        );
    }
}

async fn assert_one_finished_event(events: &MemoryEventStore, run_id: &str, expected: RunStatus) {
    let finished: Vec<_> = events
        .list_since(run_id, None, 100)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == RunEventType::RunFinished)
        .collect();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].run_status, Some(expected));
}

async fn wait_for_terminal(store: &dyn StateStore, run_id: &str) -> RunInfo {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let info = store.get_run_info(run_id).await.unwrap();
            if info.status.is_terminal() {
                return info;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("run did not become terminal")
}

#[tokio::test]
async fn state_write_failure_stalls_run_and_repairs_running_task() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultStore::new(dir.path()));
    store.fail_next_success_task_write();
    let events = Arc::new(MemoryEventStore::new());
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(ImmediateNode));
    let engine =
        WorkflowEngine::new_with_events(Arc::new(registry), store.clone(), events.clone(), None);
    let flow = FlowDefinition {
        name: "state_failure".to_string(),
        steps: vec![step("work", "test_immediate", &[])],
    };

    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    let error = handle.wait().await.unwrap_err().to_string();

    assert!(error.contains("stalled"));
    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Stalled);
    assert_eq!(info.tasks["work"].status, TaskStatus::Failed);
    assert_fully_terminal(&info);
    assert_one_finished_event(&events, &run_id, RunStatus::Stalled).await;
}

#[tokio::test]
async fn infrastructure_failure_discards_completed_sibling_phase_output() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultStore::new(dir.path()));
    store.fail_next_success_task_write_for("faulting");
    let (faulting, signals) = ControlledNode::new("test_faulting", None);
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(ImmediateNode));
    registry.register(Arc::new(faulting));
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), Some(2));
    let flow = FlowDefinition {
        name: "phase_infrastructure_failure".to_string(),
        steps: vec![
            step("fast", "test_immediate", &[]),
            step("faulting", "test_faulting", &[]),
        ],
    };

    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    signals.started.await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let info = store.get_run_info(&run_id).await.unwrap();
            if info.tasks["fast"].status == TaskStatus::Success {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("fast sibling did not complete");

    signals.release.notify_one();
    handle.wait().await.unwrap_err();
    signals.dropped.await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Stalled);
    assert_eq!(info.tasks["fast"].status, TaskStatus::Success);
    assert_eq!(info.tasks["fast"].output.as_ref().unwrap()["done"], true);
    assert_eq!(info.tasks["faulting"].status, TaskStatus::Failed);
    assert!(!info.ctx.contains_key("done"));
    assert!(!info.ctx.contains_key("controlled_done"));
    assert_fully_terminal(&info);
}

#[tokio::test]
async fn terminal_status_write_is_retried_before_returning() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultStore::new(dir.path()));
    store.fail_next_terminal_status_write();
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(ImmediateNode));
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), None);
    let flow = FlowDefinition {
        name: "terminal_retry".to_string(),
        steps: vec![step("work", "test_immediate", &[])],
    };

    let run_id = engine.execute(&flow, Context::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(store.terminal_attempts(), 2);
    assert_eq!(info.status, RunStatus::Success);
    assert_fully_terminal(&info);
}

#[tokio::test]
async fn final_context_failure_still_persists_stalled_status() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultStore::new(dir.path()));
    store.fail_next_context_write();
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(ImmediateNode));
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), None);
    let flow = FlowDefinition {
        name: "context_failure".to_string(),
        steps: vec![step("work", "test_immediate", &[])],
    };

    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    handle.wait().await.unwrap_err();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Stalled);
    assert_eq!(info.tasks["work"].status, TaskStatus::Success);
    assert_fully_terminal(&info);
}

#[tokio::test]
async fn node_panic_stalls_run_and_drops_parallel_and_pending_work() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultStore::new(dir.path()));
    let events = Arc::new(MemoryEventStore::new());
    let barrier = Arc::new(Barrier::new(2));
    let (blocking, signals) = ControlledNode::new("test_blocking", Some(barrier.clone()));
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(PanicNode::new(barrier)));
    registry.register(Arc::new(blocking));
    registry.register(Arc::new(ImmediateNode));
    let engine =
        WorkflowEngine::new_with_events(Arc::new(registry), store.clone(), events.clone(), Some(2));
    let flow = FlowDefinition {
        name: "panic".to_string(),
        steps: vec![
            step("panic", "test_panic", &[]),
            step("blocking", "test_blocking", &[]),
            step("after", "test_immediate", &["panic", "blocking"]),
        ],
    };

    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    tokio::time::timeout(Duration::from_secs(10), signals.started)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), handle.wait())
        .await
        .unwrap()
        .unwrap_err();
    tokio::time::timeout(Duration::from_secs(10), signals.dropped)
        .await
        .unwrap()
        .unwrap();

    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Stalled);
    assert_eq!(info.tasks["panic"].status, TaskStatus::Failed);
    assert_eq!(info.tasks["blocking"].status, TaskStatus::Failed);
    assert_eq!(info.tasks["after"].status, TaskStatus::Skipped);
    assert_fully_terminal(&info);
    assert_one_finished_event(&events, &run_id, RunStatus::Stalled).await;
}

#[tokio::test]
async fn explicit_cancellation_persists_cancelled_run_and_task() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultStore::new(dir.path()));
    let events = Arc::new(MemoryEventStore::new());
    let (blocking, signals) = ControlledNode::new("test_blocking", None);
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(blocking));
    let engine =
        WorkflowEngine::new_with_events(Arc::new(registry), store.clone(), events.clone(), None);
    let flow = FlowDefinition {
        name: "cancel".to_string(),
        steps: vec![step("blocking", "test_blocking", &[])],
    };

    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    signals.started.await.unwrap();
    assert_eq!(handle.cancel().await.unwrap(), run_id);
    signals.dropped.await.unwrap();

    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Cancelled);
    assert_eq!(info.tasks["blocking"].status, TaskStatus::Cancelled);
    assert_fully_terminal(&info);
    assert_one_finished_event(&events, &run_id, RunStatus::Cancelled).await;
}

#[tokio::test]
async fn dropped_waiter_detaches_without_abandoning_run() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FaultStore::new(dir.path()));
    let (blocking, signals) = ControlledNode::new("test_blocking", None);
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(blocking));
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), None);
    let flow = FlowDefinition {
        name: "detached".to_string(),
        steps: vec![step("blocking", "test_blocking", &[])],
    };

    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    let waiter = tokio::spawn(handle.wait());
    signals.started.await.unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    signals.release.notify_one();
    signals.dropped.await.unwrap();

    let info = wait_for_terminal(store.as_ref(), &run_id).await;
    assert_eq!(info.status, RunStatus::Success);
    assert_fully_terminal(&info);
}
