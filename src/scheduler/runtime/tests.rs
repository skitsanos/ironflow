use super::*;

fn task(handle: JoinHandle<()>, shutdown: oneshot::Sender<()>) -> SchedulerTask {
    task_in(handle, shutdown, crate::api::ServiceLifecycle::default())
}

fn task_in(
    handle: JoinHandle<()>,
    shutdown: oneshot::Sender<()>,
    lifecycle: crate::api::ServiceLifecycle,
) -> SchedulerTask {
    SchedulerTask {
        shutdown: Some(shutdown),
        handle,
        lifecycle,
    }
}

#[tokio::test]
async fn an_unexpected_task_exit_fails_supervision() {
    let (shutdown, _receiver) = oneshot::channel();
    let scheduler = task(tokio::spawn(async {}), shutdown);
    let result = scheduler
        .supervise(std::future::pending::<Result<()>>())
        .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("stopped unexpectedly")
    );
}

#[tokio::test]
async fn server_completion_requests_and_awaits_scheduler_shutdown() {
    let (shutdown, receiver) = oneshot::channel();
    let handle = tokio::spawn(async move {
        receiver.await.unwrap();
    });
    task(handle, shutdown)
        .supervise(async { Ok(()) })
        .await
        .unwrap();
}

#[tokio::test]
async fn a_panicking_task_fails_supervision() {
    let (shutdown, _receiver) = oneshot::channel();
    let scheduler = task(
        tokio::spawn(async { panic!("injected scheduler panic") }),
        shutdown,
    );
    let error = scheduler
        .supervise(std::future::pending::<Result<()>>())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("panicked"), "{error}");
}

struct GatedNode {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl crate::nodes::Node for GatedNode {
    fn node_type(&self) -> &str {
        "test_gated"
    }

    fn description(&self) -> &str {
        "Wait until the test releases the step"
    }

    async fn execute(
        &self,
        _: &serde_json::Value,
        _: &crate::engine::types::Context,
    ) -> Result<crate::engine::types::NodeOutput> {
        self.started.notify_one();
        self.release.notified().await;
        Ok(crate::engine::types::NodeOutput::new())
    }
}

/// IF-203 (e): a dead scheduler task stops `serve`, but accepted runs are
/// drained first, exactly as they are on a shutdown signal.
#[tokio::test]
async fn a_dead_scheduler_drains_active_runs_before_failing_supervision() {
    use crate::engine::types::{Context, FlowDefinition, RunStatus, StepDefinition};
    use crate::engine::{RetryConfig, WorkflowEngine};
    use crate::storage::null_store::NullStateStore;

    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let mut registry = crate::nodes::NodeRegistry::new();
    registry.register(Arc::new(GatedNode {
        started: started.clone(),
        release: release.clone(),
    }));
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), Some(1));
    let flow = FlowDefinition {
        name: "accepted".to_string(),
        steps: vec![StepDefinition {
            name: "work".to_string(),
            node_type: "test_gated".to_string(),
            config: serde_json::Value::Null,
            dependencies: Vec::new(),
            retry: RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        }],
    };
    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    let lifecycle = crate::api::ServiceLifecycle::default();
    let active = lifecycle.track(&handle);
    let waiter = tokio::spawn(async move {
        let _active = active;
        handle.wait().await
    });
    started.notified().await;

    let (shutdown, _receiver) = oneshot::channel();
    let scheduler = task_in(tokio::spawn(async {}), shutdown, lifecycle.clone());
    let supervision = tokio::spawn(scheduler.supervise(std::future::pending::<Result<()>>()));

    // Admission closes at once; the accepted run keeps supervision open.
    lifecycle.wait_for_draining().await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        !supervision.is_finished(),
        "supervision returned while an accepted run was still executing"
    );
    assert_eq!(lifecycle.active_runs(), 1);

    release.notify_one();
    let error = supervision.await.unwrap().unwrap_err().to_string();
    assert!(error.contains("stopped unexpectedly"), "{error}");
    assert_eq!(lifecycle.active_runs(), 0);
    waiter.await.unwrap().unwrap();
    assert_eq!(
        crate::storage::StateStore::get_run_info(store.as_ref(), &run_id)
            .await
            .unwrap()
            .status,
        RunStatus::Success
    );
}

#[tokio::test]
async fn an_expired_grace_cancels_the_run_and_still_fails_supervision() {
    use crate::engine::types::{Context, FlowDefinition, RunStatus, StepDefinition};
    use crate::engine::{RetryConfig, WorkflowEngine};
    use crate::storage::null_store::NullStateStore;

    let started = Arc::new(tokio::sync::Notify::new());
    let mut registry = crate::nodes::NodeRegistry::new();
    registry.register(Arc::new(GatedNode {
        started: started.clone(),
        release: Arc::new(tokio::sync::Notify::new()),
    }));
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), Some(1));
    let flow = FlowDefinition {
        name: "accepted".to_string(),
        steps: vec![StepDefinition {
            name: "work".to_string(),
            node_type: "test_gated".to_string(),
            config: serde_json::Value::Null,
            dependencies: Vec::new(),
            retry: RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        }],
    };
    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    let lifecycle = crate::api::ServiceLifecycle::default();
    let active = lifecycle.track(&handle);
    let waiter = tokio::spawn(async move {
        let _active = active;
        handle.wait().await
    });
    started.notified().await;

    let (shutdown, _receiver) = oneshot::channel();
    let error = task_in(tokio::spawn(async {}), shutdown, lifecycle.clone())
        .supervise_with_grace(std::future::pending::<Result<()>>(), Duration::ZERO)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("stopped unexpectedly"), "{error}");
    assert_eq!(lifecycle.active_runs(), 0);
    let _ = waiter.await.unwrap();
    assert_eq!(
        crate::storage::StateStore::get_run_info(store.as_ref(), &run_id)
            .await
            .unwrap()
            .status,
        RunStatus::Cancelled
    );
}
