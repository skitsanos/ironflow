use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::executor::WorkflowEngine;
use crate::engine::types::{Context, NodeOutput, RunStatus};
use crate::lua::runtime::LuaRuntime;
use crate::nodes::{Node, NodeRegistry};
use crate::storage::null_store::NullStateStore;

pub struct SubworkflowNode {
    /// Registry containing all non-subworkflow nodes.
    /// At execution time, we add ourselves to give children full capabilities.
    pub base_registry: Arc<NodeRegistry>,
}

impl SubworkflowNode {
    /// Build a full registry for child execution by adding subworkflow support.
    fn child_registry(&self) -> Arc<NodeRegistry> {
        let mut child = self.base_registry.snapshot();
        child.register(Arc::new(SubworkflowNode {
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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let flow_file = config
            .get("flow")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("subworkflow requires 'flow' parameter"))?;

        let wait = config.get("wait").and_then(|v| v.as_bool()).unwrap_or(true);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

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

        // Build a full registry (with subworkflow support) for the child engine
        let child_registry = self.child_registry();

        // Load the subworkflow
        let flow = LuaRuntime::load_flow(&flow_path_str, &child_registry)?;

        let store: Arc<dyn crate::storage::StateStore> = Arc::new(NullStateStore::new());

        if wait {
            let engine = WorkflowEngine::new(child_registry, store.clone(), None);
            let run_id = engine.execute(&flow, sub_ctx).await?;
            let run_info = store.get_run_info(&run_id).await?;

            let child_succeeded = matches!(run_info.status, RunStatus::Success);

            // If the child flow failed and no output_key is set, propagate as error
            if !child_succeeded && output_key.is_none() {
                return Err(anyhow::anyhow!(
                    "Subworkflow '{}' finished with status: {}",
                    flow.name,
                    run_info.status
                ));
            }

            let mut output = NodeOutput::new();

            if let Some(ref key) = output_key {
                output.insert(key.clone(), serde_json::to_value(&run_info.ctx)?);
                output.insert(
                    format!("{}_success", key),
                    serde_json::Value::Bool(child_succeeded),
                );
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

            Ok(output)
        } else {
            // Fire-and-forget — spawn in background
            let flow_name = flow.name.clone();
            let flow_name2 = flow_name.clone();
            tokio::spawn(async move {
                let engine = WorkflowEngine::new(child_registry, store, None);
                if let Err(e) = engine.execute(&flow, sub_ctx).await {
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
