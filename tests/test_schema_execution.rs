#[path = "schema_refs/worker_gate.rs"]
mod worker_gate;

use std::sync::Arc;
use std::time::Duration;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{
    Context, FlowDefinition, RetryConfig, RunStatus, StepDefinition, TaskStatus,
};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::{StateStore, null_store::NullStateStore};
use ironflow::util::execution::with_execution_deadline;
use serde_json::json;
use tracing_subscriber::prelude::*;

#[tokio::test]
async fn schema_nodes_reject_expired_deadlines_before_compilation() {
    for name in ["validate_schema", "json_validate"] {
        let node = NodeRegistry::with_builtins().get(name).unwrap();
        let error = with_execution_deadline(
            Some(tokio::time::Instant::now()),
            node.execute(
                &json!({"source_key": "data", "schema": {"type": "integer"}}),
                &Context::from([("data".into(), json!(1))]),
            ),
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("step deadline exceeded"),
            "{error}"
        );
    }
}

// A current-thread runtime proves the synchronous gate is on the blocking pool:
// the timer and cancellation future must keep making progress while it is held.
#[tokio::test]
async fn cancellation_and_timeout_retain_admission_until_schema_workers_exit() {
    let layer = worker_gate::GateLayer::default();
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(layer.clone()))
        .unwrap();
    for name in ["validate_schema", "json_validate"] {
        for timeout in [false, true] {
            let (release, started, cancelled) = layer.arm();
            let store = Arc::new(NullStateStore::new());
            let engine = WorkflowEngine::new(
                Arc::new(NodeRegistry::with_builtins()),
                store.clone(),
                Some(1),
            );
            let flow = FlowDefinition {
                name: "schema-worker".into(),
                steps: vec![StepDefinition {
                    name: "validate".into(),
                    node_type: name.into(),
                    config: json!({"source_key": "data", "schema": {"type": "integer"}}),
                    dependencies: vec![],
                    retry: RetryConfig::default(),
                    timeout_s: timeout.then_some(2.0),
                    route: None,
                    on_error: None,
                }],
            };
            let admission = Arc::new(tokio::sync::Semaphore::new(1));
            let permit = admission.clone().acquire_owned().await.unwrap();
            let handle = engine
                .start(&flow, Context::from([("data".into(), json!(1))]))
                .await
                .unwrap();
            let id = handle.id().to_owned();
            tokio::time::timeout(Duration::from_secs(5), started)
                .await
                .expect("schema worker never started")
                .unwrap();
            let completion = tokio::spawn(async move {
                let _permit = permit;
                if timeout {
                    handle.wait().await
                } else {
                    handle.cancel().await
                }
            });
            tokio::time::timeout(Duration::from_secs(5), cancelled)
                .await
                .expect("schema worker did not receive cancellation")
                .unwrap();
            assert!(
                !completion.is_finished(),
                "run settled before its worker exited"
            );
            assert!(
                admission.try_acquire().is_err(),
                "admission released before worker exit"
            );
            release.0.release();
            tokio::time::timeout(Duration::from_secs(5), completion)
                .await
                .expect("schema worker did not drain")
                .unwrap()
                .unwrap();
            let _permit = admission.try_acquire().expect("admission was not released");
            let info = store.get_run_info(&id).await.unwrap();
            assert_eq!(
                info.status,
                if timeout {
                    RunStatus::Failed
                } else {
                    RunStatus::Cancelled
                }
            );
            assert_eq!(
                info.tasks["validate"].status,
                if timeout {
                    TaskStatus::Failed
                } else {
                    TaskStatus::Cancelled
                }
            );
            assert!(!info.ctx.contains_key("validation_success"));
            if timeout {
                assert!(
                    info.tasks["validate"]
                        .error
                        .as_deref()
                        .unwrap()
                        .contains("timed out")
                );
            }
        }
    }
}
