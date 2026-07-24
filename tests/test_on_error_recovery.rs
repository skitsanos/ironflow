//! Integration coverage for planned `on_error` recovery execution.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, ensure};
use async_trait::async_trait;
use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{
    Context, FlowDefinition, NodeOutput, RetryConfig, RunInfo, RunStatus, StepDefinition,
    TaskStatus,
};
use ironflow::nodes::{Node, NodeRegistry};
use ironflow::storage::StateStore;
use ironflow::storage::null_store::NullStateStore;
use serde_json::{Value, json};
use tokio::sync::Barrier;

struct FailingNode;

#[async_trait]
impl Node for FailingNode {
    fn node_type(&self) -> &str {
        "if007_fail"
    }

    fn description(&self) -> &str {
        "Fails with a test-controlled message"
    }

    async fn execute(&self, config: &Value, _ctx: &Context) -> Result<NodeOutput> {
        anyhow::bail!("{}", config["message"].as_str().unwrap_or("IF-007 failure"))
    }
}

struct ConcurrentRecoveryNode {
    barrier: Arc<Barrier>,
}

#[async_trait]
impl Node for ConcurrentRecoveryNode {
    fn node_type(&self) -> &str {
        "if007_concurrent_recovery"
    }

    fn description(&self) -> &str {
        "Checks invocation-local recovery metadata"
    }

    async fn execute(&self, config: &Value, ctx: &Context) -> Result<NodeOutput> {
        tokio::time::timeout(Duration::from_secs(2), self.barrier.wait())
            .await
            .context("recovery handlers did not execute concurrently")?;

        let expected_step = required_config(config, "expected_step")?;
        let expected_message = required_config(config, "expected_message")?;
        let output_prefix = required_config(config, "output_prefix")?;
        let actual_step = required_context(ctx, "_error_step")?;
        let actual_message = required_context(ctx, "_error_message")?;
        let actual_node_type = required_context(ctx, "_error_node_type")?;

        ensure!(actual_step == expected_step, "wrong recovery source");
        ensure!(
            actual_message.contains(expected_message),
            "wrong recovery error message"
        );
        ensure!(actual_node_type == "if007_fail", "wrong recovery node type");

        Ok(HashMap::from([
            (format!("{output_prefix}_step"), json!(actual_step)),
            (format!("{output_prefix}_message"), json!(actual_message)),
            (
                format!("{output_prefix}_node_type"),
                json!(actual_node_type),
            ),
        ]))
    }
}

fn required_config<'a>(config: &'a Value, key: &str) -> Result<&'a str> {
    config[key]
        .as_str()
        .with_context(|| format!("missing test config '{key}'"))
}

fn required_context<'a>(ctx: &'a Context, key: &str) -> Result<&'a str> {
    ctx.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("missing recovery context '{key}'"))
}

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

fn flow(name: &str, steps: Vec<StepDefinition>) -> FlowDefinition {
    FlowDefinition {
        name: name.to_string(),
        steps,
    }
}

fn test_engine(barrier: Option<Arc<Barrier>>) -> (WorkflowEngine, Arc<dyn StateStore>) {
    let mut registry = NodeRegistry::with_builtins();
    registry.register(Arc::new(FailingNode));
    if let Some(barrier) = barrier {
        registry.register(Arc::new(ConcurrentRecoveryNode { barrier }));
    }
    let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), Some(4));
    (engine, store)
}

async fn execute(flow: FlowDefinition, barrier: Option<Arc<Barrier>>) -> RunInfo {
    let (engine, store) = test_engine(barrier);
    let run_id = engine.execute(&flow, Context::new()).await.unwrap();
    store.get_run_info(&run_id).await.unwrap()
}

fn assert_invalid(flow: FlowDefinition, expected_fragments: &[&str]) {
    let errors = flow.validate_dag().join("\n").to_lowercase();
    assert!(!errors.is_empty(), "flow unexpectedly passed validation");
    for fragment in expected_fragments {
        assert!(
            errors.contains(&fragment.to_lowercase()),
            "validation errors did not contain '{fragment}': {errors}"
        );
    }
}

#[test]
fn validates_recovery_target_ownership_and_metadata() {
    let mut source = step("source", "log", json!({ "message": "source" }));
    source.on_error = Some("missing".to_string());
    assert_invalid(
        flow("missing", vec![source]),
        &["missing", "does not exist"],
    );

    let mut source = step("source", "log", json!({ "message": "source" }));
    source.on_error = Some("source".to_string());
    assert_invalid(flow("self", vec![source]), &["source", "itself"]);

    let mut first = step("first", "log", json!({ "message": "first" }));
    let mut second = step("second", "log", json!({ "message": "second" }));
    first.on_error = Some("handler".to_string());
    second.on_error = Some("handler".to_string());
    assert_invalid(
        flow(
            "shared",
            vec![first, second, step("handler", "log", json!({}))],
        ),
        &["handler", "multiple"],
    );

    let mut source = step("source", "log", json!({}));
    let mut handler = step("handler", "log", json!({}));
    source.on_error = Some("handler".to_string());
    handler.on_error = Some("fallback".to_string());
    assert_invalid(
        flow(
            "nested",
            vec![source, handler, step("fallback", "log", json!({}))],
        ),
        &["handler", "on_error"],
    );

    let mut source = step("source", "log", json!({}));
    let mut handler = step("handler", "log", json!({}));
    source.on_error = Some("handler".to_string());
    handler.route = Some("recovery".to_string());
    assert_invalid(flow("route", vec![source, handler]), &["handler", "route"]);

    let mut source = step("source", "log", json!({}));
    let mut handler = step("handler", "log", json!({}));
    source.on_error = Some("handler".to_string());
    handler.dependencies.push("source".to_string());
    assert_invalid(
        flow("source_dependency", vec![source, handler]),
        &["handler", "depend", "source"],
    );
}

#[test]
fn validates_cycles_in_the_augmented_recovery_graph() {
    let mut source = step("source", "log", json!({}));
    let mut downstream = step("downstream", "log", json!({}));
    let mut handler = step("handler", "log", json!({}));
    source.on_error = Some("handler".to_string());
    downstream.dependencies.push("source".to_string());
    handler.dependencies.push("downstream".to_string());

    let errors = flow("recovery_cycle", vec![source, downstream, handler]).validate_dag();
    assert!(!errors.is_empty());
    assert!(errors.join("\n").to_lowercase().contains("cycle"));
}

#[tokio::test]
async fn successful_recovery_resolves_run_but_preserves_source_failure() {
    let mut source = step(
        "source",
        "if007_fail",
        json!({ "message": "source failed" }),
    );
    source.on_error = Some("handler".to_string());
    let handler = step(
        "handler",
        "code",
        json!({ "source": "return { recovered = true }" }),
    );
    let mut downstream = step(
        "downstream",
        "code",
        json!({ "source": "assert(ctx.recovered); return { continued = true }" }),
    );
    downstream.dependencies.push("source".to_string());

    let info = execute(
        flow("successful_recovery", vec![source, handler, downstream]),
        None,
    )
    .await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks["source"].status, TaskStatus::Failed);
    assert_eq!(info.tasks["handler"].status, TaskStatus::Success);
    assert_eq!(info.tasks["downstream"].status, TaskStatus::Success);
    assert_eq!(info.ctx["recovered"], json!(true));
    assert_eq!(info.ctx["continued"], json!(true));
    assert!(info.tasks["downstream"].started.unwrap() >= info.tasks["handler"].finished.unwrap());
}

#[tokio::test]
async fn recovery_handler_waits_for_its_declared_dependencies() {
    let mut source = step(
        "source",
        "if007_fail",
        json!({ "message": "source failed" }),
    );
    source.on_error = Some("handler".to_string());
    let prepare = step("prepare", "delay", json!({ "seconds": 0.05 }));
    let mut handler = step(
        "handler",
        "code",
        json!({ "source": "return { recovered = true }" }),
    );
    handler.dependencies.push("prepare".to_string());

    let info = execute(
        flow("recovery_dependencies", vec![source, prepare, handler]),
        None,
    )
    .await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks["handler"].status, TaskStatus::Success);
    assert!(info.tasks["handler"].started.unwrap() >= info.tasks["prepare"].finished.unwrap());
}

#[tokio::test]
async fn failed_recovery_preserves_failure_status_and_attempt_count() {
    let mut source = step(
        "source",
        "if007_fail",
        json!({ "message": "source failed" }),
    );
    source.on_error = Some("handler".to_string());
    let mut handler = step(
        "handler",
        "if007_fail",
        json!({ "message": "handler failed" }),
    );
    handler.retry.max_retries = 1;
    handler.retry.backoff_s = 0.0;

    let info = execute(flow("failed_recovery", vec![source, handler]), None).await;

    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.tasks["source"].status, TaskStatus::Failed);
    assert_eq!(info.tasks["handler"].status, TaskStatus::Failed);
    assert_eq!(info.tasks["handler"].attempt, 2);
    assert!(
        info.tasks["handler"]
            .error
            .as_deref()
            .unwrap()
            .contains("handler failed")
    );
}

#[tokio::test]
async fn untriggered_recovery_branch_skips_without_failing_the_run() {
    let mut source = step(
        "source",
        "code",
        json!({ "source": "return { source_succeeded = true }" }),
    );
    source.on_error = Some("handler".to_string());
    let handler = step(
        "handler",
        "code",
        json!({ "source": "return { handler_ran = true }" }),
    );
    let mut recovery_branch = step(
        "recovery_branch",
        "code",
        json!({ "source": "return { branch_ran = true }" }),
    );
    recovery_branch.dependencies.push("handler".to_string());

    let info = execute(
        flow("untriggered", vec![source, handler, recovery_branch]),
        None,
    )
    .await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks["source"].status, TaskStatus::Success);
    assert_eq!(info.tasks["handler"].status, TaskStatus::Skipped);
    assert_eq!(info.tasks["recovery_branch"].status, TaskStatus::Skipped);
    assert!(!info.ctx.contains_key("handler_ran"));
    assert!(!info.ctx.contains_key("branch_ran"));
}

#[tokio::test]
async fn concurrent_handlers_receive_local_metadata_without_global_leaks() {
    let mut source_a = step("source_a", "if007_fail", json!({ "message": "failure-a" }));
    let mut source_b = step("source_b", "if007_fail", json!({ "message": "failure-b" }));
    source_a.on_error = Some("handler_a".to_string());
    source_b.on_error = Some("handler_b".to_string());
    let handler_a = step(
        "handler_a",
        "if007_concurrent_recovery",
        json!({
            "expected_step": "source_a",
            "expected_message": "failure-a",
            "output_prefix": "a"
        }),
    );
    let handler_b = step(
        "handler_b",
        "if007_concurrent_recovery",
        json!({
            "expected_step": "source_b",
            "expected_message": "failure-b",
            "output_prefix": "b"
        }),
    );

    let info = execute(
        flow(
            "concurrent_recovery",
            vec![source_a, source_b, handler_a, handler_b],
        ),
        Some(Arc::new(Barrier::new(2))),
    )
    .await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx["a_step"], json!("source_a"));
    assert_eq!(info.ctx["b_step"], json!("source_b"));
    assert_eq!(info.ctx["a_node_type"], json!("if007_fail"));
    assert_eq!(info.ctx["b_node_type"], json!("if007_fail"));
    assert!(
        info.ctx["a_message"]
            .as_str()
            .unwrap()
            .contains("failure-a")
    );
    assert!(
        info.ctx["b_message"]
            .as_str()
            .unwrap()
            .contains("failure-b")
    );
    assert!(!info.ctx.contains_key("_error_step"));
    assert!(!info.ctx.contains_key("_error_message"));
    assert!(!info.ctx.contains_key("_error_node_type"));
}
