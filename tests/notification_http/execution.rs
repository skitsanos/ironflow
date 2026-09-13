use std::sync::Arc;
use std::time::Duration;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{
    Context, FlowDefinition, RetryConfig, RunStatus, StepDefinition, TaskStatus,
};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::{StateStore, null_store::NullStateStore};

use super::support::{Server, config, isolated};

#[tokio::test]
async fn cancellation_and_deadlines_during_body_reads_leave_no_outputs() {
    if isolated("execution::cancellation_and_deadlines_during_body_reads_leave_no_outputs").await {
        return;
    }
    for node in ["slack_notification", "send_email"] {
        for deadline in [false, true] {
            let server =
                Server::start(200, "text/plain", vec![b"ok".to_vec()], None, true, false).await;
            let store = Arc::new(NullStateStore::new());
            let engine = WorkflowEngine::new(
                Arc::new(NodeRegistry::with_builtins()),
                store.clone(),
                Some(1),
            );
            let flow = FlowDefinition {
                name: "notification-cancel".into(),
                steps: vec![StepDefinition {
                    name: "notify".into(),
                    node_type: node.into(),
                    config: config(node, &server.url),
                    dependencies: vec![],
                    retry: RetryConfig::default(),
                    timeout_s: deadline.then_some(1.0),
                    route: None,
                    on_error: None,
                }],
            };
            let handle = engine.start(&flow, Context::new()).await.unwrap();
            let id = handle.id().to_owned();
            tokio::time::timeout(Duration::from_secs(5), server.started.notified())
                .await
                .expect("provider did not start streaming");
            tokio::time::timeout(Duration::from_secs(3), async {
                if deadline {
                    handle.wait().await
                } else {
                    handle.cancel().await
                }
            })
            .await
            .expect("notification did not settle")
            .unwrap();
            let run = store.get_run_info(&id).await.unwrap();
            assert_eq!(
                run.status,
                if deadline {
                    RunStatus::Failed
                } else {
                    RunStatus::Cancelled
                }
            );
            assert_eq!(
                run.tasks["notify"].status,
                if deadline {
                    TaskStatus::Failed
                } else {
                    TaskStatus::Cancelled
                }
            );
            super::assert_no_output(&serde_json::to_value(&run.ctx).unwrap(), "result");
            if deadline {
                assert!(
                    run.tasks["notify"]
                        .error
                        .as_deref()
                        .unwrap()
                        .contains("timed out")
                );
            }
        }
    }
}
