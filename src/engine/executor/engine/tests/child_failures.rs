use super::*;
use crate::engine::types::{RetryConfig, RunStatus, StepDefinition, TaskStatus};
use serde_json::json;

fn step(name: &str, node_type: &str, config: serde_json::Value) -> StepDefinition {
    StepDefinition {
        name: name.to_string(),
        node_type: node_type.to_string(),
        config,
        dependencies: Vec::new(),
        retry: RetryConfig::default(),
        timeout_s: None,
        route: None,
        on_error: None,
    }
}

#[tokio::test]
async fn child_failures_exclude_recovered_tasks_even_when_history_still_marks_them_failed() {
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(Arc::new(NodeRegistry::with_builtins()), store.clone(), None);
    for unrecovered in [false, true] {
        let mut source = step(
            "source",
            "code",
            json!({"source": "error('recovered-boom')"}),
        );
        source.on_error = Some("recover".to_string());
        let mut steps = vec![
            source,
            step(
                "recover",
                "code",
                json!({"source": "return { recovered = true }"}),
            ),
        ];
        if unrecovered {
            steps.push(step(
                "other",
                "code",
                json!({"source": "error('unresolved-boom')"}),
            ));
        }
        let flow = FlowDefinition {
            name: "recovery".to_string(),
            steps,
        };
        let id = Uuid::new_v4().to_string();
        let result = engine
            .start_new_with_run_id(
                &flow,
                Context::new(),
                ExecutionOverlay::default(),
                id.clone(),
                true,
            )
            .await
            .unwrap()
            .unwrap()
            .wait_child_cancel_on_drop()
            .await
            .unwrap();
        let info = store.get_run_info(&id).await.unwrap();
        assert_eq!(info.tasks["source"].status, TaskStatus::Failed);
        assert_eq!(result.ctx["recovered"], true);
        if unrecovered {
            assert_eq!(result.status, RunStatus::Failed);
            let summary = result.failure_summary.unwrap();
            assert!(summary.contains("unresolved-boom"), "{summary}");
            assert!(!summary.contains("recovered-boom"), "{summary}");
            assert!(!summary.contains("task 'source'"), "{summary}");
        } else {
            assert_eq!(result.status, RunStatus::Success);
            assert!(result.failure_summary.is_none());
        }
    }
}

#[tokio::test]
async fn child_failures_are_redacted_through_every_composition_consumer() {
    let directory = tempfile::tempdir().unwrap();
    let child = directory.path().join("child.lua");
    std::fs::write(&child, r#"
        local flow = Flow.new("secret-child")
        flow:step("fail", function(ctx)
            error("boom " .. ctx._headers.authorization .. " https://user:password@invalid.test/?api_key=url-secret")
        end)
        return flow
    "#).unwrap();
    let secret = "if135-overlay-secret";
    let overlay = ExecutionOverlay::new(Context::from([(
        "_headers".to_string(),
        json!({"authorization": secret}),
    )]));
    let configurations = [
        ("subworkflow", json!({"flow": child, "on_error": "ignore"})),
        (
            "subworkflow",
            json!({"flow": child, "on_error": "ignore", "output_key": "child"}),
        ),
        (
            "subworkflow",
            json!({"flow": child, "on_error": "fail_fast"}),
        ),
        (
            "parallel_subworkflows",
            json!({"flows": [{"flow": child}], "on_error": "ignore"}),
        ),
        (
            "parallel_subworkflows",
            json!({"flows": [{"flow": child}], "on_error": "fail_fast"}),
        ),
        (
            "repeat_subworkflow",
            json!({"flow": child, "max_iterations": 2}),
        ),
        (
            "tool_dispatch",
            json!({"source_key": "calls", "tools": {"tool": {"flow": child}}, "on_error": "ignore"}),
        ),
        (
            "tool_dispatch",
            json!({"source_key": "calls", "tools": {"tool": {"flow": child}}, "on_error": "fail_fast"}),
        ),
    ];
    let engine = WorkflowEngine::new(
        Arc::new(NodeRegistry::with_builtins()),
        Arc::new(NullStateStore::new()),
        None,
    );
    for (node_type, config) in configurations {
        let ignore = config["on_error"] == "ignore";
        let flow = FlowDefinition {
            name: format!("parent-{node_type}"),
            steps: vec![step("invoke", node_type, config)],
        };
        let ctx = Context::from([(
            "calls".to_string(),
            json!([{
                "id": "call-1", "name": "tool", "arguments": {}, "raw_arguments": "{}"
            }]),
        )]);
        let result = engine
            .execute_child(&flow, ctx, overlay.clone())
            .await
            .unwrap();
        assert_eq!(
            result.status,
            if ignore {
                RunStatus::Success
            } else {
                RunStatus::Failed
            }
        );
        let text = if ignore {
            assert!(result.failure_summary.is_none());
            serde_json::to_string(&result.ctx).unwrap()
        } else {
            result.failure_summary.unwrap()
        };
        assert!(text.contains("boom"), "{node_type}: {text}");
        assert!(text.contains("[REDACTED]"), "{node_type}: {text}");
        for forbidden in [secret, "user:password", "url-secret"] {
            assert!(!text.contains(forbidden), "{node_type}: {text}");
        }
    }
}

struct WaitingNode(Arc<tokio::sync::Notify>);

#[async_trait]
impl crate::nodes::Node for WaitingNode {
    fn node_type(&self) -> &str {
        "if135_wait"
    }
    fn description(&self) -> &str {
        "Wait for cancellation"
    }
    async fn execute(&self, _: &serde_json::Value, _: &Context) -> Result<Context> {
        self.0.notify_one();
        std::future::pending().await
    }
}

#[tokio::test]
async fn cancelled_child_has_no_task_failure_summary() {
    let started = Arc::new(tokio::sync::Notify::new());
    let mut registry = NodeRegistry::with_builtins();
    registry.register(Arc::new(WaitingNode(started.clone())));
    let engine = WorkflowEngine::new(Arc::new(registry), Arc::new(NullStateStore::new()), None);
    let flow = FlowDefinition {
        name: "cancelled-child".to_string(),
        steps: vec![step("wait", "if135_wait", json!({}))],
    };
    let handle = engine
        .start_new_with_run_id(
            &flow,
            Context::new(),
            ExecutionOverlay::default(),
            Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), started.notified())
        .await
        .unwrap();
    handle.cancellation().request();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        handle.wait_child_cancel_on_drop(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.status, RunStatus::Cancelled);
    assert!(result.failure_summary.is_none());
    assert_eq!(result.terminal_reason(), "finished with status: cancelled");
}
