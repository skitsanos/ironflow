use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use async_trait::async_trait;

use crate::engine::executor::ExecutionOverlay;
use crate::engine::types::{Context, NodeOutput};
use crate::nodes::{Node, NodeRegistry};

use super::parallel_runner::{ChildRun, run_children};
use crate::util::node_config::config_usize_strict;

mod inputs;
use inputs::resolve_flow_entries;

/// Hard cap on `max_concurrent` to guard against pathological config values.
const MAX_PARALLEL_SUBWORKFLOWS_CAP: usize = 1024;

pub struct ParallelSubworkflowsNode {
    /// Registry containing all non-subworkflow nodes.
    /// At execution time, we add subworkflow support to give children full capabilities.
    pub base_registry: Arc<NodeRegistry>,
}

impl ParallelSubworkflowsNode {
    fn child_registry(&self) -> Arc<NodeRegistry> {
        super::registry::child_registry(&self.base_registry)
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

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let entries = resolve_flow_entries(config, ctx)?;

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

        // Concurrency cap. Default: num_cpus. Users can raise or lower it per
        // node. Hard-capped at MAX_PARALLEL_SUBWORKFLOWS_CAP to block pathological
        // config values from saturating the runtime.
        let max_concurrent =
            config_usize_strict(config, "max_concurrent", ctx)?.unwrap_or_else(num_cpus::get);
        if max_concurrent == 0 {
            anyhow::bail!("parallel_subworkflows: 'max_concurrent' must be greater than 0");
        }
        let max_concurrent = max_concurrent.min(MAX_PARALLEL_SUBWORKFLOWS_CAP);

        let child_registry = self.child_registry();
        let execution_overlay = ExecutionOverlay::current();

        // Resolve flow_dir from context
        let flow_dir = ctx
            .get("_flow_dir")
            .and_then(|v| v.as_str())
            .map(String::from);

        let mut children = Vec::with_capacity(entries.len());
        let mut flows = Vec::with_capacity(entries.len());

        for (idx, entry) in entries.into_iter().enumerate() {
            let flow_file = entry
                .config
                .get("flow")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "parallel_subworkflows: each flow entry requires a 'flow' field (index {})",
                        idx
                    )
                })?
                .to_string();

            let (flow_cfg, mut sub_ctx) = entry.into_parts(ctx);

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
                .with_context(|| {
                    format!(
                        "parallel_subworkflows: cannot find '{}'",
                        flow_path.display()
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
            execution_overlay.strip_from_context(&mut sub_ctx);

            flows.push(flow_cfg);
            children.push(ChildRun {
                index: idx,
                flow_path: flow_path_str,
                context: sub_ctx,
                execution_overlay: execution_overlay.clone(),
            });
        }

        let completed = run_children(children, &flows, child_registry, max_concurrent).await?;
        let results = completed.results;
        let errors = completed.errors;

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

        output.insert(output_key.to_string(), serde_json::Value::Array(results));
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
