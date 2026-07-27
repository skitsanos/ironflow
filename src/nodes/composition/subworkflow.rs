use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Semaphore;

use crate::engine::executor::ExecutionOverlay;
use crate::engine::executor::WorkflowEngine;
use crate::engine::types::{Context, NodeOutput, RunStatus};
use crate::lua::runtime::LuaRuntime;
use crate::nodes::{Node, NodeRegistry};
use crate::storage::null_store::NullStateStore;
use crate::util::node_config::config_bool;

/// Process-global cap on concurrently-running fire-and-forget subworkflows.
/// Override with `IRONFLOW_MAX_DETACHED_SUBWORKFLOWS`.
const DEFAULT_MAX_DETACHED_SUBWORKFLOWS: usize = 64;

pub(crate) fn detached_subworkflow_capacity() -> usize {
    std::env::var("IRONFLOW_MAX_DETACHED_SUBWORKFLOWS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MAX_DETACHED_SUBWORKFLOWS)
}

pub(crate) fn detached_subworkflow_semaphore() -> &'static Arc<Semaphore> {
    use std::sync::OnceLock;
    static SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SEMAPHORE.get_or_init(|| Arc::new(Semaphore::new(detached_subworkflow_capacity())))
}

pub struct SubworkflowNode {
    /// Registry containing all non-subworkflow nodes.
    /// At execution time, we add ourselves to give children full capabilities.
    pub base_registry: Arc<NodeRegistry>,
}

impl SubworkflowNode {
    /// Build a full registry for child execution by adding subworkflow +
    /// parallel_subworkflows support, so nested flows can also compose.
    fn child_registry(&self) -> Arc<NodeRegistry> {
        let mut child = self.base_registry.snapshot();
        child.register(Arc::new(SubworkflowNode {
            base_registry: self.base_registry.clone(),
        }));
        child.register(Arc::new(
            super::parallel_subworkflows::ParallelSubworkflowsNode {
                base_registry: self.base_registry.clone(),
            },
        ));
        child.register(Arc::new(super::tool_dispatch::ToolDispatchNode {
            base_registry: self.base_registry.clone(),
        }));
        Arc::new(child)
    }
}

#[async_trait]
impl Node for SubworkflowNode {
    fn node_type(&self) -> &str {
        "subworkflow"
    }

    fn description(&self) -> &str {
        "Load and execute another .lua flow as a reusable module"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let flow_file = config
            .get("flow")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("subworkflow requires 'flow' parameter"))?;

        let wait = config_bool(config, "wait", ctx).unwrap_or(true);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Error policy, matching `parallel_subworkflows`. When unset we keep the
        // historical behaviour, in which `output_key` implicitly decided it:
        // namespaced output tolerated a failed child, un-namespaced output
        // propagated the error. Those are orthogonal concerns and the coupling
        // is easy to miss, so `on_error` lets a flow state the policy directly.
        let fail_fast = match config.get("on_error").and_then(|v| v.as_str()) {
            Some("fail_fast") => true,
            Some("ignore") => false,
            None => output_key.is_none(),
            Some(other) => {
                return Err(anyhow::anyhow!(
                    "subworkflow: invalid on_error '{}'; expected 'fail_fast' or 'ignore'",
                    other
                ));
            }
        };

        // Resolve the flow path relative to _flow_dir
        let flow_path = if PathBuf::from(flow_file).is_absolute() {
            PathBuf::from(flow_file)
        } else {
            let flow_dir = ctx
                .get("_flow_dir")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "subworkflow: cannot resolve relative path '{}' — _flow_dir not set",
                        flow_file
                    )
                })?;
            PathBuf::from(flow_dir).join(flow_file)
        };

        let flow_path_str = flow_path
            .canonicalize()
            .map_err(|e| {
                anyhow::anyhow!("subworkflow: cannot find '{}': {}", flow_path.display(), e)
            })?
            .to_string_lossy()
            .to_string();

        // Build subworkflow context from input mapping or full parent context
        let mut sub_ctx = if let Some(input_map) = config.get("input").and_then(|v| v.as_object()) {
            let mut mapped = Context::new();
            for (sub_key, parent_key_val) in input_map {
                if let Some(parent_key) = parent_key_val.as_str() {
                    if let Some(value) = ctx.get(parent_key) {
                        mapped.insert(sub_key.clone(), value.clone());
                    }
                } else {
                    // Direct value (not a key reference)
                    mapped.insert(sub_key.clone(), parent_key_val.clone());
                }
            }
            mapped
        } else {
            ctx.clone()
        };

        // Set _flow_dir for the subworkflow (enables nested subworkflows)
        if let Some(parent) = PathBuf::from(&flow_path_str).parent() {
            sub_ctx.insert(
                "_flow_dir".to_string(),
                serde_json::Value::String(parent.to_string_lossy().to_string()),
            );
        }

        let execution_overlay = ExecutionOverlay::current();
        execution_overlay.strip_from_context(&mut sub_ctx);

        // Build a full registry (with subworkflow support) for the child engine
        let child_registry = self.child_registry();

        // Load the subworkflow
        let flow = LuaRuntime::load_flow_async(&flow_path_str, &child_registry).await?;

        let store: Arc<dyn crate::storage::StateStore> = Arc::new(NullStateStore::new());

        if wait {
            let engine = WorkflowEngine::new(child_registry, store.clone(), None);
            let run_id = engine
                .start_with_execution_overlay(&flow, sub_ctx, execution_overlay)
                .await?
                .wait_cancel_on_drop()
                .await?;
            let run_info = store.get_run_info(&run_id).await?;

            let child_succeeded = matches!(run_info.status, RunStatus::Success);

            if !child_succeeded && fail_fast {
                return Err(anyhow::anyhow!(
                    "Subworkflow '{}' finished with status: {}",
                    flow.name,
                    run_info.status
                ));
            }

            // A tolerated failure is otherwise invisible in the parent's log —
            // only the child run records it — which makes a silently empty
            // result hard to trace back. Say so, and expose it on the context.
            if !child_succeeded {
                tracing::warn!(
                    flow = %flow.name,
                    status = %run_info.status,
                    output_key = output_key.as_deref().unwrap_or("<none>"),
                    "Subworkflow failed but on_error=ignore; continuing with its partial context"
                );
            }

            let mut output = NodeOutput::new();

            if let Some(ref key) = output_key {
                output.insert(key.clone(), serde_json::to_value(&run_info.ctx)?);
                output.insert(
                    format!("{}_success", key),
                    serde_json::Value::Bool(child_succeeded),
                );
                if !child_succeeded {
                    output.insert(
                        format!("{}_error", key),
                        serde_json::Value::String(format!(
                            "Subworkflow '{}' finished with status: {}",
                            flow.name, run_info.status
                        )),
                    );
                }
            } else {
                // Merge subworkflow output directly into parent context
                for (k, v) in run_info.ctx.iter() {
                    if !k.starts_with('_') {
                        output.insert(k.to_string(), v.clone());
                    }
                }
            }

            output.insert(
                "subworkflow_name".to_string(),
                serde_json::Value::String(flow.name),
            );
            // Always reported, so a flow can check the outcome even when it did
            // not namespace the child's output with `output_key`.
            output.insert(
                "subworkflow_success".to_string(),
                serde_json::Value::Bool(child_succeeded),
            );

            Ok(output)
        } else {
            // Fire-and-forget — spawn in background under a process-global
            // semaphore so a caller cannot fan out unbounded background work.
            let permit = detached_subworkflow_semaphore()
                .clone()
                .try_acquire_owned()
                .map_err(|_| {
                    anyhow::anyhow!(
                        "subworkflow: detached fan-out limit reached ({}); refusing to spawn. \
                         Raise IRONFLOW_MAX_DETACHED_SUBWORKFLOWS or wait for in-flight flows to complete.",
                        detached_subworkflow_capacity()
                    )
                })?;

            let flow_name = flow.name.clone();
            let flow_name2 = flow_name.clone();
            tokio::spawn(async move {
                // Permit is dropped when this task exits, releasing one slot.
                let _permit = permit;
                let engine = WorkflowEngine::new(child_registry, store, None);
                let result = engine
                    .start_with_execution_overlay(&flow, sub_ctx, execution_overlay)
                    .await;
                let result = match result {
                    Ok(handle) => handle.wait().await.map(|_| ()),
                    Err(error) => Err(error),
                };
                if let Err(e) = result {
                    tracing::error!(
                        flow = %flow_name,
                        error = %e,
                        "Background subworkflow failed"
                    );
                }
            });

            let mut output = NodeOutput::new();
            output.insert(
                "subworkflow_name".to_string(),
                serde_json::Value::String(flow_name2),
            );
            output.insert(
                "subworkflow_async".to_string(),
                serde_json::Value::Bool(true),
            );
            Ok(output)
        }
    }
}
