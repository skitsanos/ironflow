//! Structured execution and result collection for parallel subworkflows.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{Map, Value};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::engine::executor::{ExecutionOverlay, WorkflowEngine};
use crate::engine::types::{Context, RunInfo, RunStatus};
use crate::lua::runtime::LuaRuntime;
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::null_store::NullStateStore;
use crate::util::execution::{current_execution_deadline, with_execution_deadline};

pub(super) struct ChildRun {
    pub(super) index: usize,
    pub(super) flow_path: String,
    pub(super) context: Context,
    pub(super) execution_overlay: ExecutionOverlay,
}

pub(super) struct ParallelRunOutput {
    pub(super) results: Vec<Value>,
    pub(super) errors: Vec<String>,
}

type ChildResult = Result<(String, RunInfo)>;
type ChildTaskOutput = (usize, ChildResult);

pub(super) async fn run_children(
    children: Vec<ChildRun>,
    flow_configs: &[Value],
    registry: Arc<NodeRegistry>,
    max_concurrent: usize,
) -> Result<ParallelRunOutput> {
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let deadline = current_execution_deadline();
    let mut tasks: JoinSet<ChildTaskOutput> = JoinSet::new();
    let mut task_indices = HashMap::with_capacity(children.len());

    for child in children {
        let index = child.index;
        let registry = registry.clone();
        let semaphore = semaphore.clone();
        let abort_handle = tasks.spawn(with_execution_deadline(deadline, async move {
            let result = run_child(child, registry, semaphore).await;
            (index, result)
        }));
        task_indices.insert(abort_handle.id(), index);
    }

    collect_children(&mut tasks, task_indices, flow_configs).await
}

async fn run_child(
    child: ChildRun,
    registry: Arc<NodeRegistry>,
    semaphore: Arc<Semaphore>,
) -> ChildResult {
    let _permit = semaphore
        .acquire_owned()
        .await
        .expect("parallel child semaphore cannot be closed");
    let flow = LuaRuntime::load_flow_async(&child.flow_path, &registry).await?;
    let flow_name = flow.name.clone();
    let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(registry, store.clone(), None);
    let run_id = engine
        .start_with_execution_overlay(&flow, child.context, child.execution_overlay)
        .await?
        .wait_cancel_on_drop()
        .await?;
    let run_info = store.get_run_info(&run_id).await?;
    Ok((flow_name, run_info))
}

async fn collect_children(
    tasks: &mut JoinSet<ChildTaskOutput>,
    mut task_indices: HashMap<tokio::task::Id, usize>,
    flow_configs: &[Value],
) -> Result<ParallelRunOutput> {
    let mut results: Vec<Option<Value>> = vec![None; flow_configs.len()];
    let mut errors: Vec<Option<String>> = vec![None; flow_configs.len()];

    while let Some(joined) = tasks.join_next_with_id().await {
        match joined {
            Ok((task_id, (index, Ok((name, run_info))))) => {
                task_indices.remove(&task_id);
                let (entry, error) = success_entry(&flow_configs[index], index, name, run_info)?;
                results[index] = Some(entry);
                errors[index] = error;
            }
            Ok((task_id, (index, Err(error)))) => {
                task_indices.remove(&task_id);
                let message = format!("Subworkflow at index {index} failed: {error}");
                results[index] = Some(failure_entry(&flow_configs[index], &message));
                errors[index] = Some(message);
            }
            Err(error) => {
                let index = task_indices.remove(&error.id()).ok_or_else(|| {
                    anyhow::anyhow!(
                        "parallel_subworkflows: lost index for failed child task {}",
                        error.id()
                    )
                })?;
                let message = format!("Subworkflow task at index {index} panicked: {error}");
                results[index] = Some(failure_entry(&flow_configs[index], &message));
                errors[index] = Some(message);
            }
        }
    }

    Ok(ParallelRunOutput {
        results: results
            .into_iter()
            .map(|result| result.unwrap_or(Value::Null))
            .collect(),
        errors: errors.into_iter().flatten().collect(),
    })
}

fn success_entry(
    flow_config: &Value,
    index: usize,
    name: String,
    run_info: RunInfo,
) -> Result<(Value, Option<String>)> {
    let succeeded = matches!(run_info.status, RunStatus::Success);
    let mut entry = Map::new();
    entry.insert("success".to_string(), Value::Bool(succeeded));
    entry.insert("flow".to_string(), Value::String(name.clone()));

    if let Some(output_key) = flow_config.get("output_key").and_then(Value::as_str) {
        entry.insert(output_key.to_string(), serde_json::to_value(&run_info.ctx)?);
    } else {
        for (key, value) in &run_info.ctx {
            if !key.starts_with('_') {
                entry.insert(key.clone(), value.clone());
            }
        }
    }

    let error = (!succeeded).then(|| {
        format!(
            "Subworkflow '{}' (index {}) finished with status: {}",
            name, index, run_info.status
        )
    });
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
