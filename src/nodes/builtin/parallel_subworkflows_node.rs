use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::executor::WorkflowEngine;
use crate::engine::types::{Context, NodeOutput, RunStatus};
use crate::lua::runtime::LuaRuntime;
use crate::nodes::{Node, NodeRegistry};
use crate::storage::null_store::NullStateStore;

use super::subworkflow_node::SubworkflowNode;

pub struct ParallelSubworkflowsNode {
    /// Registry containing all non-subworkflow nodes.
    /// At execution time, we add subworkflow support to give children full capabilities.
    pub base_registry: Arc<NodeRegistry>,
}

impl ParallelSubworkflowsNode {
    /// Build a full registry for child execution by adding subworkflow +
    /// parallel_subworkflows support.
    fn child_registry(&self) -> Arc<NodeRegistry> {
        let mut child = self.base_registry.snapshot();
        child.register(Arc::new(SubworkflowNode {
            base_registry: self.base_registry.clone(),
        }));
        child.register(Arc::new(ParallelSubworkflowsNode {
            base_registry: self.base_registry.clone(),
        }));
        Arc::new(child)
    }
}

#[async_trait]
impl Node for ParallelSubworkflowsNode {
    fn node_type(&self) -> &str {
        "parallel_subworkflows"
    }

    fn description(&self) -> &str {
        "Execute multiple subworkflows concurrently and collect their results"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let flows = config
            .get("flows")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                anyhow::anyhow!("parallel_subworkflows requires 'flows' array parameter")
            })?;

        if flows.is_empty() {
            return Err(anyhow::anyhow!(
                "parallel_subworkflows: 'flows' array must not be empty"
            ));
        }

        let fail_fast = match config
            .get("on_error")
            .and_then(|v| v.as_str())
            .unwrap_or("fail_fast")
        {
            "fail_fast" => true,
            "ignore" => false,
            other => {
                return Err(anyhow::anyhow!(
                    "parallel_subworkflows: invalid on_error '{}'; expected 'fail_fast' or 'ignore'",
                    other
                ));
            }
        };

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("parallel_results");

        let child_registry = self.child_registry();

        // Resolve flow_dir from context
        let flow_dir = ctx
            .get("_flow_dir")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Spawn one tokio task per subworkflow
        let mut handles = Vec::with_capacity(flows.len());

        for (idx, flow_cfg) in flows.iter().enumerate() {
            let flow_file = flow_cfg
                .get("flow")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "parallel_subworkflows: each flow entry requires a 'flow' field (index {})",
                        idx
                    )
                })?
                .to_string();

            // Build child context from input mapping or clone parent
            let mut sub_ctx =
                if let Some(input_map) = flow_cfg.get("input").and_then(|v| v.as_object()) {
                    let mut mapped = Context::new();
                    for (sub_key, parent_key_val) in input_map {
                        if let Some(parent_key) = parent_key_val.as_str() {
                            if let Some(value) = ctx.get(parent_key) {
                                mapped.insert(sub_key.clone(), value.clone());
                            } else {
                                mapped.insert(
                                    sub_key.clone(),
                                    serde_json::Value::String(parent_key.to_string()),
                                );
                            }
                        } else {
                            mapped.insert(sub_key.clone(), parent_key_val.clone());
                        }
                    }
                    mapped
                } else {
                    ctx.clone()
                };

            // Resolve flow path
            let flow_path = if PathBuf::from(&flow_file).is_absolute() {
                PathBuf::from(&flow_file)
            } else {
                let dir = flow_dir.as_deref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "parallel_subworkflows: cannot resolve relative path '{}' — _flow_dir not set",
                        flow_file
                    )
                })?;
                PathBuf::from(dir).join(&flow_file)
            };

            let flow_path_str = flow_path
                .canonicalize()
                .map_err(|e| {
                    anyhow::anyhow!(
                        "parallel_subworkflows: cannot find '{}': {}",
                        flow_path.display(),
                        e
                    )
                })?
                .to_string_lossy()
                .to_string();

            // Set _flow_dir for nested subworkflows
            if let Some(parent) = PathBuf::from(&flow_path_str).parent() {
                sub_ctx.insert(
                    "_flow_dir".to_string(),
                    serde_json::Value::String(parent.to_string_lossy().to_string()),
                );
            }

            let registry = child_registry.clone();

            let handle = tokio::spawn(async move {
                let flow = LuaRuntime::load_flow(&flow_path_str, &registry)?;
                let flow_name = flow.name.clone();
                let store: Arc<dyn crate::storage::StateStore> = Arc::new(NullStateStore::new());
                let engine = WorkflowEngine::new(registry, store.clone(), None);
                let run_id = engine.execute(&flow, sub_ctx).await?;
                let run_info = store.get_run_info(&run_id).await?;
                Ok::<_, anyhow::Error>((idx, flow_name, run_info))
            });

            handles.push(handle);
        }

        // Collect results
        let mut results: Vec<Option<serde_json::Value>> = vec![None; flows.len()];
        let mut flow_names: Vec<String> = vec![String::new(); flows.len()];
        let mut errors: Vec<String> = Vec::new();

        // Handles are pushed in order, so enumerate gives us the matching flow index
        for (i, handle) in handles.into_iter().enumerate() {
            match handle.await {
                Ok(Ok((idx, name, run_info))) => {
                    flow_names[idx] = name;
                    let succeeded = matches!(run_info.status, RunStatus::Success);

                    let per_flow_key = flows[idx]
                        .get("output_key")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let mut entry = serde_json::Map::new();
                    entry.insert("success".to_string(), serde_json::Value::Bool(succeeded));
                    entry.insert(
                        "flow".to_string(),
                        serde_json::Value::String(flow_names[idx].clone()),
                    );

                    if let Some(key) = per_flow_key {
                        entry.insert(key, serde_json::to_value(&run_info.ctx)?);
                    } else {
                        for (k, v) in &run_info.ctx {
                            if !k.starts_with('_') {
                                entry.insert(k.clone(), v.clone());
                            }
                        }
                    }

                    if !succeeded {
                        let msg = format!(
                            "Subworkflow '{}' (index {}) finished with status: {}",
                            flow_names[idx], idx, run_info.status
                        );
                        entry.insert("error".to_string(), serde_json::Value::String(msg.clone()));
                        errors.push(msg);
                    }

                    results[idx] = Some(serde_json::Value::Object(entry));
                }
                Ok(Err(e)) => {
                    let msg = format!("Subworkflow at index {} failed: {}", i, e);
                    errors.push(msg.clone());

                    let flow_name = flows[i]
                        .get("flow")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    let mut entry = serde_json::Map::new();
                    entry.insert("success".to_string(), serde_json::Value::Bool(false));
                    entry.insert(
                        "flow".to_string(),
                        serde_json::Value::String(flow_name.to_string()),
                    );
                    entry.insert("error".to_string(), serde_json::Value::String(msg));
                    results[i] = Some(serde_json::Value::Object(entry));
                }
                Err(e) => {
                    let msg = format!("Subworkflow task at index {} panicked: {}", i, e);
                    errors.push(msg.clone());

                    let mut entry = serde_json::Map::new();
                    entry.insert("success".to_string(), serde_json::Value::Bool(false));
                    entry.insert(
                        "flow".to_string(),
                        serde_json::Value::String(
                            flows[i]
                                .get("flow")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string(),
                        ),
                    );
                    entry.insert("error".to_string(), serde_json::Value::String(msg));
                    results[i] = Some(serde_json::Value::Object(entry));
                }
            }
        }

        // Handle error policy
        if !errors.is_empty() && fail_fast {
            return Err(anyhow::anyhow!(
                "parallel_subworkflows: {} flow(s) failed:\n{}",
                errors.len(),
                errors.join("\n")
            ));
        }

        // Build output
        let mut output = NodeOutput::new();

        let results_array: Vec<serde_json::Value> = results
            .into_iter()
            .map(|r| r.unwrap_or(serde_json::Value::Null))
            .collect();

        output.insert(
            output_key.to_string(),
            serde_json::Value::Array(results_array),
        );
        output.insert(
            format!("{}_count", output_key),
            serde_json::Value::Number(flows.len().into()),
        );
        output.insert(
            format!("{}_errors", output_key),
            serde_json::Value::Number(errors.len().into()),
        );
        output.insert(
            format!("{}_all_succeeded", output_key),
            serde_json::Value::Bool(errors.is_empty()),
        );

        Ok(output)
    }
}
