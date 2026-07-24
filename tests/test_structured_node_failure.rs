//! Executor coverage for terminal structured node failures.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use async_trait::async_trait;
use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{
    Context, FlowDefinition, NodeOutput, RetryConfig, RunInfo, RunStatus, StepDefinition,
    TaskStatus,
};
use ironflow::nodes::{Node, NodeFailure, NodeRegistry};
use ironflow::storage::StateStore;
use ironflow::storage::null_store::NullStateStore;
use serde_json::{Value, json};

fn step(name: &str, node_type: &str, config: Value) -> StepDefinition {
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

async fn execute(registry: NodeRegistry, steps: Vec<StepDefinition>) -> RunInfo {
    execute_with_concurrency(registry, steps, 1).await
}

async fn execute_with_concurrency(
    registry: NodeRegistry,
    steps: Vec<StepDefinition>,
    concurrency: usize,
) -> RunInfo {
    let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), Some(concurrency));
    let flow = FlowDefinition {
        name: "structured_failure".to_string(),
        steps,
    };
    let run_id = engine.execute(&flow, Context::new()).await.unwrap();
    store.get_run_info(&run_id).await.unwrap()
}

struct CollisionFailureNode;

#[async_trait]
impl Node for CollisionFailureNode {
    fn node_type(&self) -> &str {
        "if019_collision_failure"
    }

    fn description(&self) -> &str {
        "Returns delayed structured output for collision testing"
    }

    async fn execute(&self, _config: &Value, _ctx: &Context) -> Result<NodeOutput> {
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        Err(NodeFailure::new(
            "controlled failure",
            Context::from([
                ("collision".to_string(), json!("failed-source")),
                ("source_diagnostic".to_string(), json!("exact")),
            ]),
        )
        .into())
    }
}

#[tokio::test]
async fn recovery_keeps_exact_failure_output_after_a_parallel_collision() {
    let mut registry = NodeRegistry::with_builtins();
    registry.register(Arc::new(CollisionFailureNode));

    let mut source = step("source", "if019_collision_failure", json!({}));
    source.on_error = Some("recover".to_string());
    let sibling = step(
        "sibling",
        "code",
        json!({"source": "return { collision = 'later-declaration' }"}),
    );
    let recover = step(
        "recover",
        "code",
        json!({
            "source": r#"
                assert(ctx.collision == "later-declaration")
                assert(ctx._error_output.collision == "failed-source")
                assert(ctx._error_output.source_diagnostic == "exact")
                return { recovered_collision = ctx._error_output.collision }
            "#
        }),
    );

    let info = execute_with_concurrency(registry, vec![source, sibling, recover], 2).await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx["collision"], "later-declaration");
    assert_eq!(info.ctx["recovered_collision"], "failed-source");
    assert_eq!(
        info.tasks["source"].output.as_ref().unwrap()["collision"],
        "failed-source"
    );
    assert!(!info.ctx.contains_key("_error_output"));
}

#[tokio::test]
async fn terminal_shell_failure_publishes_diagnostics() {
    let source = step(
        "source",
        "shell_command",
        json!({
            "cmd": "sh",
            "args": ["-c", "printf completed; printf rejected >&2; exit 7"],
            "output_key": "command"
        }),
    );

    let info = execute(NodeRegistry::with_builtins(), vec![source]).await;
    let task = &info.tasks["source"];
    let task_output = task.output.as_ref().unwrap();

    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(task_output["command_stdout"], "completed");
    assert_eq!(task_output["command_stderr"], "rejected");
    assert_eq!(task_output["command_code"], 7);
    assert_eq!(task_output["command_success"], false);
    assert_eq!(info.ctx["command_stderr"], "rejected");
    assert!(!task.error.as_deref().unwrap().contains("rejected"));
}

#[tokio::test]
async fn recovery_receives_exact_structured_failure_output() {
    let mut source = step(
        "source",
        "shell_command",
        json!({
            "cmd": "sh",
            "args": ["-c", "printf recoverable >&2; exit 5"],
            "output_key": "command"
        }),
    );
    source.on_error = Some("recover".to_string());
    let recover = step(
        "recover",
        "code",
        json!({
            "source": r#"
                assert(ctx.command_code == 5)
                assert(ctx.command_stderr == "recoverable")
                assert(ctx._error_output.command_code == 5)
                assert(ctx._error_output.command_stderr == "recoverable")
                return { recovered = true }
            "#
        }),
    );

    let info = execute(NodeRegistry::with_builtins(), vec![source, recover]).await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks["source"].status, TaskStatus::Failed);
    assert_eq!(info.tasks["recover"].status, TaskStatus::Success);
    assert_eq!(info.ctx["recovered"], true);
    assert!(!info.ctx.contains_key("_error_output"));
}

#[tokio::test]
async fn nonzero_opt_out_is_successful_and_skips_recovery() {
    let mut source = step(
        "source",
        "shell_command",
        json!({
            "cmd": "sh",
            "args": ["-c", "printf inspectable >&2; exit 3"],
            "fail_on_nonzero": false
        }),
    );
    source.retry.max_retries = 2;
    source.retry.backoff_s = 0.0;
    source.on_error = Some("recover".to_string());
    let recover = step(
        "recover",
        "code",
        json!({"source": "return { recovery_ran = true }"}),
    );

    let info = execute(NodeRegistry::with_builtins(), vec![source, recover]).await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks["source"].status, TaskStatus::Success);
    assert_eq!(info.tasks["source"].attempt, 1);
    assert_eq!(info.tasks["recover"].status, TaskStatus::Skipped);
    assert_eq!(info.ctx["shell_success"], false);
    assert_eq!(info.ctx["shell_code"], 3);
    assert!(!info.ctx.contains_key("recovery_ran"));
}

struct RetryNode {
    attempts: AtomicUsize,
    succeed_on: Option<usize>,
}

#[async_trait]
impl Node for RetryNode {
    fn node_type(&self) -> &str {
        "if022_retry"
    }

    fn description(&self) -> &str {
        "Returns attempt-tagged structured failures"
    }

    async fn execute(&self, _config: &Value, ctx: &Context) -> Result<NodeOutput> {
        assert!(
            !ctx.contains_key("attempt_diagnostic"),
            "retry observed output from an intermediate failure"
        );
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if self.succeed_on == Some(attempt) {
            return Ok(HashMap::from([("result".to_string(), json!("success"))]));
        }

        Err(NodeFailure::new(
            format!("attempt {attempt} failed"),
            HashMap::from([(
                "attempt_diagnostic".to_string(),
                json!(format!("attempt-{attempt}")),
            )]),
        )
        .into())
    }
}

fn retry_step() -> StepDefinition {
    let mut source = step("source", "if022_retry", json!({}));
    source.retry.max_retries = 1;
    source.retry.backoff_s = 0.0;
    source
}

#[tokio::test]
async fn successful_retry_does_not_publish_intermediate_failure() {
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(RetryNode {
        attempts: AtomicUsize::new(0),
        succeed_on: Some(2),
    }));

    let info = execute(registry, vec![retry_step()]).await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks["source"].attempt, 2);
    assert_eq!(
        info.tasks["source"].output.as_ref().unwrap()["result"],
        "success"
    );
    assert_eq!(info.ctx["result"], "success");
    assert!(!info.ctx.contains_key("attempt_diagnostic"));
}

#[tokio::test]
async fn exhausted_retries_publish_only_the_final_attempt() {
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(RetryNode {
        attempts: AtomicUsize::new(0),
        succeed_on: None,
    }));

    let info = execute(registry, vec![retry_step()]).await;

    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.tasks["source"].attempt, 2);
    assert_eq!(
        info.tasks["source"].output.as_ref().unwrap()["attempt_diagnostic"],
        "attempt-2"
    );
    assert_eq!(info.ctx["attempt_diagnostic"], "attempt-2");
}

#[tokio::test]
async fn backoff_timeout_keeps_the_last_completed_attempt_diagnostics() {
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(RetryNode {
        attempts: AtomicUsize::new(0),
        succeed_on: None,
    }));
    let mut source = retry_step();
    source.retry.max_retries = 3;
    source.retry.backoff_s = 0.2;
    source.timeout_s = Some(0.03);

    let info = execute(registry, vec![source]).await;

    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.tasks["source"].attempt, 1);
    assert_eq!(
        info.tasks["source"].error.as_deref(),
        Some("Task 'source' timed out after 0.03s total")
    );
    assert_eq!(
        info.tasks["source"].output.as_ref().unwrap()["attempt_diagnostic"],
        "attempt-1"
    );
    assert_eq!(info.ctx["attempt_diagnostic"], "attempt-1");
}
