use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::executor::{ChildLink, ExecutionOverlay};
use crate::engine::types::{Context, NodeOutput};
use crate::nodes::{Node, NodeRegistry};

use super::parallel_runner::{FanOut, run_children};
use crate::util::node_config::config_usize_strict;

mod inputs;
mod plan;
pub(super) use inputs::FlowEntry;
use inputs::resolve_flow_entries;
use plan::plan_children;

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
        let link = ChildLink::for_current_step("parallel_subworkflows")?;
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

        // Concurrency cap. Default: the host's available parallelism. Users can
        // raise or lower it per
        // node. Hard-capped at MAX_PARALLEL_SUBWORKFLOWS_CAP to block pathological
        // config values from saturating the runtime.
        let max_concurrent = config_usize_strict(config, "max_concurrent", ctx)?
            .unwrap_or_else(crate::util::runtime_config::host_parallelism);
        if max_concurrent == 0 {
            anyhow::bail!("parallel_subworkflows: 'max_concurrent' must be greater than 0");
        }
        let max_concurrent = max_concurrent.min(MAX_PARALLEL_SUBWORKFLOWS_CAP);

        // Every flow path is resolved before any child starts, so a missing
        // flow still fails the step without running its siblings.
        let flow_dir = ctx.get("_flow_dir").and_then(|v| v.as_str());
        let plans = plan_children(entries, flow_dir).await?;
        let flow_count = plans.len();

        let fan_out = FanOut {
            parent: ctx,
            registry: self.child_registry(),
            overlay: ExecutionOverlay::current(),
            link,
        };
        let completed = run_children(plans, &fan_out, max_concurrent).await?;
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
            serde_json::Value::Number(flow_count.into()),
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
