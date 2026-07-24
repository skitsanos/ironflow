//! Regression coverage for deterministic phase context publication.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Result, bail};
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

struct ConfiguredWriter;

#[async_trait]
impl Node for ConfiguredWriter {
    fn node_type(&self) -> &str {
        "if019_writer"
    }

    fn description(&self) -> &str {
        "Test writer with controlled completion timing"
    }

    async fn execute(&self, config: &Value, ctx: &Context) -> Result<NodeOutput> {
        if let Some(key) = config.get("expect_absent").and_then(Value::as_str)
            && ctx.contains_key(key)
        {
            bail!("independent step unexpectedly observed peer key '{key}'");
        }
        if let Some(expectation) = config.get("expect") {
            let key = expectation["key"].as_str().unwrap();
            let expected = &expectation["value"];
            if ctx.get(key) != Some(expected) {
                bail!("expected context key '{key}' to equal {expected}");
            }
        }

        let delay_ms = config.get("delay_ms").and_then(Value::as_u64).unwrap_or(0);
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        serde_json::from_value(config["output"].clone()).map_err(Into::into)
    }
}

struct RetryIsolationProbe {
    attempts: AtomicUsize,
}

#[async_trait]
impl Node for RetryIsolationProbe {
    fn node_type(&self) -> &str {
        "if019_retry_probe"
    }

    fn description(&self) -> &str {
        "Checks that retries retain their phase-start context"
    }

    async fn execute(&self, _config: &Value, ctx: &Context) -> Result<NodeOutput> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt == 1 {
            bail!("retry once");
        }
        if ctx.contains_key("peer_value") {
            bail!("retry observed an independent peer's same-phase output");
        }
        Ok(Context::from([("isolated".to_string(), json!(true))]))
    }
}

fn step(name: &str, node_type: &str, config: Value, dependencies: &[&str]) -> StepDefinition {
    StepDefinition {
        name: name.to_string(),
        node_type: node_type.to_string(),
        config,
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

fn registry() -> NodeRegistry {
    let mut registry = NodeRegistry::new();
    registry.register(Arc::new(ConfiguredWriter));
    registry
}

async fn execute(
    registry: NodeRegistry,
    steps: Vec<StepDefinition>,
    concurrency: usize,
) -> RunInfo {
    let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), Some(concurrency));
    let flow = FlowDefinition {
        name: "if019_context".to_string(),
        steps,
    };
    let run_id = engine.execute(&flow, Context::new()).await.unwrap();
    store.get_run_info(&run_id).await.unwrap()
}

#[tokio::test]
async fn later_declaration_wins_despite_reverse_completion_order() {
    let steps = vec![
        step(
            "z_first",
            "if019_writer",
            json!({
                "delay_ms": 12,
                "output": {"collision": "first", "first_only": true}
            }),
            &[],
        ),
        step(
            "a_second",
            "if019_writer",
            json!({
                "delay_ms": 0,
                "output": {"collision": "second", "second_only": true}
            }),
            &[],
        ),
    ];

    for _ in 0..25 {
        let info = execute(registry(), steps.clone(), 2).await;

        assert_eq!(info.status, RunStatus::Success);
        assert_eq!(info.ctx["collision"], "second");
        assert_eq!(info.ctx["first_only"], true);
        assert_eq!(info.ctx["second_only"], true);
        assert_eq!(
            info.tasks["z_first"].output.as_ref().unwrap()["collision"],
            "first"
        );
        assert_eq!(
            info.tasks["a_second"].output.as_ref().unwrap()["collision"],
            "second"
        );
    }
}

#[tokio::test]
async fn semaphore_serialization_does_not_create_implicit_data_flow() {
    let writer = step(
        "a_writer",
        "if019_writer",
        json!({"output": {"peer_value": "published"}}),
        &[],
    );
    let reader = step(
        "z_reader",
        "if019_writer",
        json!({
            "expect_absent": "peer_value",
            "output": {"reader_was_isolated": true}
        }),
        &[],
    );

    let info = execute(registry(), vec![writer, reader], 1).await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx["peer_value"], "published");
    assert_eq!(info.ctx["reader_was_isolated"], true);
}

#[tokio::test]
async fn retry_cannot_observe_an_independent_peer_output() {
    let mut registry = registry();
    registry.register(Arc::new(RetryIsolationProbe {
        attempts: AtomicUsize::new(0),
    }));
    let mut retry = step("retry_probe", "if019_retry_probe", json!({}), &[]);
    retry.retry = RetryConfig {
        max_retries: 1,
        backoff_s: 0.05,
    };
    let writer = step(
        "writer",
        "if019_writer",
        json!({"delay_ms": 2, "output": {"peer_value": "published"}}),
        &[],
    );

    let info = execute(registry, vec![retry, writer], 2).await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks["retry_probe"].attempt, 2);
    assert_eq!(info.ctx["isolated"], true);
    assert_eq!(info.ctx["peer_value"], "published");
}

#[tokio::test]
async fn dependent_phase_can_read_and_overwrite_an_existing_key() {
    let first = step(
        "z_first",
        "if019_writer",
        json!({"output": {"value": "first"}}),
        &[],
    );
    let second = step(
        "a_dependent",
        "if019_writer",
        json!({
            "expect": {"key": "value", "value": "first"},
            "output": {"value": "dependent"}
        }),
        &["z_first"],
    );

    let info = execute(registry(), vec![first, second], 2).await;

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx["value"], "dependent");
}

#[tokio::test]
async fn cancellation_does_not_publish_a_partial_phase() {
    let registry = Arc::new(registry());
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(registry, store.clone(), Some(2));
    let flow = FlowDefinition {
        name: "if019_cancel".to_string(),
        steps: vec![
            step(
                "fast",
                "if019_writer",
                json!({"output": {"fast_value": true}}),
                &[],
            ),
            step(
                "slow",
                "if019_writer",
                json!({"delay_ms": 10_000, "output": {"slow_value": true}}),
                &[],
            ),
        ],
    };
    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let info = store.get_run_info(&run_id).await.unwrap();
            if info
                .tasks
                .get("fast")
                .is_some_and(|task| task.status == TaskStatus::Success)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("fast phase member did not complete");

    handle.cancel().await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Cancelled);
    assert_eq!(info.tasks["fast"].status, TaskStatus::Success);
    assert_eq!(info.tasks["slow"].status, TaskStatus::Cancelled);
    assert!(!info.ctx.contains_key("fast_value"));
    assert!(!info.ctx.contains_key("slow_value"));
}
