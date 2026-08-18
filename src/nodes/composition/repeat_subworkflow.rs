mod config;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as AnyhowContext, Result};
use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::engine::executor::{ExecutionOverlay, WorkflowEngine};
use crate::engine::types::{Context, FlowDefinition, NodeOutput, RunInfo, RunStatus};
use crate::lua::runtime::LuaRuntime;
use crate::nodes::{Node, NodeRegistry};
use crate::storage::StateStore;
use crate::storage::null_store::NullStateStore;

use config::RepeatConfig;

pub struct RepeatSubworkflowNode {
    /// Registry containing all nodes except registry-backed composition nodes.
    pub base_registry: Arc<NodeRegistry>,
}

#[async_trait]
impl Node for RepeatSubworkflowNode {
    fn node_type(&self) -> &str {
        "repeat_subworkflow"
    }

    fn description(&self) -> &str {
        "Repeat a child workflow with explicit bounded state until it completes"
    }

    async fn execute(&self, config: &Value, ctx: &Context) -> Result<NodeOutput> {
        let settings = RepeatConfig::resolve(config, ctx)?;
        let unresolved = settings.unresolved_flow_path(ctx)?;
        let flow_path = tokio::fs::canonicalize(&unresolved)
            .await
            .with_context(|| {
                format!("repeat_subworkflow: cannot find '{}'", unresolved.display())
            })?;
        let flow_path = flow_path.to_string_lossy().to_string();

        let overlay = ExecutionOverlay::current();
        let mut static_context = settings.initial_context(config, ctx)?;
        overlay.strip_from_context(&mut static_context);
        let mut state = static_context.remove(&settings.state_key);
        if let Some(parent) = std::path::Path::new(&flow_path).parent() {
            static_context.insert(
                "_flow_dir".to_string(),
                Value::String(parent.to_string_lossy().to_string()),
            );
        }

        let registry = super::registry::child_registry(&self.base_registry);
        let flow = LuaRuntime::load_flow_async(&flow_path, &registry).await?;
        let mut delay = settings.delay_seconds;

        for iteration in 1..=settings.max_iterations {
            let child_context =
                iteration_context(&settings, &static_context, state.as_ref(), iteration);
            let run =
                run_iteration(&flow, child_context, registry.clone(), overlay.clone()).await?;
            ensure_child_succeeded(&flow, iteration, &run)?;

            let completed = completion_value(&settings, iteration, &run.ctx)?;
            let next_state = run.ctx.get(&settings.next_state_key).cloned();
            if completed {
                let final_state = next_state.or(state).unwrap_or(Value::Null);
                return Ok(success_output(&settings, iteration, final_state, run));
            }

            state = Some(next_state.ok_or_else(|| {
                anyhow::anyhow!(
                    "repeat_subworkflow: child '{}' iteration {} returned false in '{}' but omitted '{}'",
                    flow.name,
                    iteration,
                    settings.until_key,
                    settings.next_state_key
                )
            })?);

            if iteration == settings.max_iterations {
                anyhow::bail!(
                    "repeat_subworkflow: child '{}' did not set '{}' to true within max_iterations ({})",
                    flow.name,
                    settings.until_key,
                    settings.max_iterations
                );
            }
            if delay > 0.0 {
                tokio::time::sleep(Duration::from_secs_f64(delay)).await;
                delay = (delay * settings.backoff_factor).min(settings.max_delay_seconds);
            }
        }

        unreachable!("positive max_iterations always enters the loop")
    }
}

fn iteration_context(
    settings: &RepeatConfig,
    static_context: &Context,
    state: Option<&Value>,
    iteration: usize,
) -> Context {
    let mut context = static_context.clone();
    if let Some(state) = state {
        context.insert(settings.state_key.clone(), state.clone());
    }
    context.insert(settings.iteration_key.clone(), Value::from(iteration));
    context
}

async fn run_iteration(
    flow: &FlowDefinition,
    context: Context,
    registry: Arc<NodeRegistry>,
    overlay: ExecutionOverlay,
) -> Result<RunInfo> {
    let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(registry, store.clone(), None);
    let run_id = engine
        .start_with_execution_overlay(flow, context, overlay)
        .await?
        .wait_cancel_on_drop()
        .await?;
    Ok(store.get_run_info(&run_id).await?)
}

fn ensure_child_succeeded(flow: &FlowDefinition, iteration: usize, run: &RunInfo) -> Result<()> {
    if matches!(run.status, RunStatus::Success) {
        return Ok(());
    }
    anyhow::bail!(
        "repeat_subworkflow: child '{}' iteration {} finished with status: {}",
        flow.name,
        iteration,
        run.status
    )
}

fn completion_value(settings: &RepeatConfig, iteration: usize, ctx: &Context) -> Result<bool> {
    ctx.get(&settings.until_key)
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "repeat_subworkflow: child iteration {} must return boolean '{}'",
                iteration,
                settings.until_key
            )
        })
}

fn success_output(
    settings: &RepeatConfig,
    iterations: usize,
    final_state: Value,
    run: RunInfo,
) -> NodeOutput {
    let public: Map<String, Value> = run
        .ctx
        .into_iter()
        .filter(|(key, _)| !key.starts_with('_'))
        .collect();
    let mut output = NodeOutput::new();
    output.insert(settings.output_key.clone(), Value::Object(public));
    output.insert(format!("{}_state", settings.output_key), final_state);
    output.insert(
        format!("{}_iterations", settings.output_key),
        Value::from(iterations),
    );
    output.insert(
        format!("{}_completed", settings.output_key),
        Value::Bool(true),
    );
    output.insert(
        format!("{}_flow", settings.output_key),
        Value::String(run.flow_name),
    );
    output
}
