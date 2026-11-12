//! Structured execution and result collection for parallel subworkflows.

use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use futures_util::FutureExt as _;
use futures_util::stream::{self, StreamExt as _};
use serde_json::{Map, Value};

use crate::engine::executor::{ChildLink, ChildRunResult, ExecutionOverlay, WorkflowEngine};
use crate::engine::types::{Context, RunStatus};
use crate::lua::runtime::LuaRuntime;
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::null_store::NullStateStore;

use super::parallel_subworkflows::FlowEntry;

#[cfg(test)]
mod probe;
#[cfg(test)]
mod tests;

const RESULT_METADATA_KEYS: [&str; 3] = ["success", "flow", "error"];

pub(super) fn validate_child_output_key(key: Option<&str>) -> Result<()> {
    if let Some(key) = key
        && RESULT_METADATA_KEYS.contains(&key)
    {
        anyhow::bail!(
            "parallel_subworkflows: child output namespace '{key}' is reserved for result metadata; choose a different output key"
        );
    }
    Ok(())
}

/// A child that has not started: where its flow is and how to build its
/// context. The context itself does not exist yet.
pub(super) struct ChildPlan {
    pub(super) index: usize,
    pub(super) flow_path: Arc<str>,
    pub(super) entry: FlowEntry,
}

/// What every child of one fan-out shares.
pub(super) struct FanOut<'a> {
    pub(super) parent: &'a Context,
    pub(super) registry: Arc<NodeRegistry>,
    pub(super) overlay: ExecutionOverlay,
    pub(super) link: ChildLink,
}

pub(super) struct ParallelRunOutput {
    pub(super) results: Vec<Value>,
    pub(super) errors: Vec<String>,
}

type ChildResult = Result<(String, ChildRunResult)>;

/// Run the children with at most `max_concurrent` alive at once.
///
/// A child future is created, and its context built, only when one of the
/// `max_concurrent` slots is free, so live child contexts are proportional to
/// the concurrency and not to the number of children (IF-188). The children
/// are polled on the calling step's task: dropping this future drops every
/// started child wait, which cancels that child.
pub(super) async fn run_children(
    plans: Vec<ChildPlan>,
    fan_out: &FanOut<'_>,
    max_concurrent: usize,
) -> Result<ParallelRunOutput> {
    let flow_configs: Vec<Arc<Value>> =
        plans.iter().map(|plan| plan.entry.config.clone()).collect();
    let mut results: Vec<Option<Value>> = vec![None; plans.len()];
    let mut errors: Vec<Option<String>> = vec![None; plans.len()];

    let mut children = stream::iter(plans)
        .map(|plan| async move {
            let index = plan.index;
            let outcome = AssertUnwindSafe(run_child(plan, fan_out))
                .catch_unwind()
                .await;
            (index, outcome)
        })
        .buffer_unordered(max_concurrent);

    while let Some((index, outcome)) = children.next().await {
        let flow_config = flow_configs[index].as_ref();
        let (entry, error) = match outcome {
            Ok(Ok((name, run_info))) => success_entry(flow_config, index, name, run_info)?,
            Ok(Err(error)) => {
                let message = format!("Subworkflow at index {index} failed: {error}");
                (failure_entry(flow_config, &message), Some(message))
            }
            Err(_) => {
                let message = format!("Subworkflow task at index {index} panicked");
                (failure_entry(flow_config, &message), Some(message))
            }
        };
        results[index] = Some(entry);
        errors[index] = error;
    }

    Ok(ParallelRunOutput {
        results: results
            .into_iter()
            .map(|result| result.unwrap_or(Value::Null))
            .collect(),
        errors: errors.into_iter().flatten().collect(),
    })
}

async fn run_child(plan: ChildPlan, fan_out: &FanOut<'_>) -> ChildResult {
    #[cfg(test)]
    let _started = probe::child_started();
    let flow = LuaRuntime::load_flow_async(&plan.flow_path, &fan_out.registry).await?;
    let flow_name = flow.name.clone();

    let mut context = plan.entry.child_context(fan_out.parent);
    // Set _flow_dir for nested subworkflows.
    if let Some(parent) = Path::new(plan.flow_path.as_ref()).parent() {
        context.insert(
            "_flow_dir".to_string(),
            Value::String(parent.to_string_lossy().to_string()),
        );
    }
    fan_out.overlay.strip_from_context(&mut context);

    let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(fan_out.registry.clone(), store, None);
    let run_info = engine
        .execute_child(&flow, context, fan_out.overlay.clone(), &fan_out.link)
        .await?;
    Ok((flow_name, run_info))
}

fn success_entry(
    flow_config: &Value,
    index: usize,
    name: String,
    run_info: ChildRunResult,
) -> Result<(Value, Option<String>)> {
    let succeeded = matches!(run_info.status, RunStatus::Success);
    let error = (!succeeded).then(|| {
        format!(
            "Subworkflow '{}' (index {}) {}",
            name,
            index,
            run_info.terminal_reason()
        )
    });
    let mut entry = Map::new();

    if let Some(output_key) = flow_config.get("output_key").and_then(Value::as_str) {
        // `_`-prefixed keys are private to the child on both branches. Without
        // this filter there is no way to keep a working value out of the parent
        // when the result is namespaced, so large intermediates (an extracted
        // document IR, a full parsed transcript) cross the boundary for every
        // item in the fan-out and can exhaust the JSON-to-Lua node budget.
        let public: Map<String, Value> = run_info
            .ctx
            .into_iter()
            .filter(|(key, _)| !key.starts_with('_'))
            .collect();
        entry.insert(output_key.to_string(), Value::Object(public));
    } else {
        for (key, value) in run_info.ctx {
            if !key.starts_with('_') && !RESULT_METADATA_KEYS.contains(&key.as_str()) {
                entry.insert(key, value);
            }
        }
    }

    entry.insert("success".to_string(), Value::Bool(succeeded));
    entry.insert("flow".to_string(), Value::String(name.clone()));

    if let Some(error) = &error {
        entry.insert("error".to_string(), Value::String(error.clone()));
    }

    Ok((Value::Object(entry), error))
}

fn failure_entry(flow_config: &Value, error: &str) -> Value {
    let flow_name = flow_config
        .get("flow")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    serde_json::json!({
        "success": false,
        "flow": flow_name,
        "error": error
    })
}
