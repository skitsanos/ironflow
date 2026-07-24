use anyhow::Result;

use crate::engine::recovery::ExecutionPlan;
use crate::engine::types::{Context, FlowDefinition};

use super::engine::WorkflowEngine;

impl WorkflowEngine {
    /// Validate and plan normal and failure-triggered execution work.
    pub(super) fn execution_plan(&self, flow: &FlowDefinition) -> Result<ExecutionPlan> {
        ExecutionPlan::build(flow)
            .map_err(|errors| anyhow::anyhow!("Invalid flow: {}", errors.join("; ")))
    }

    /// Check if a step's route condition is satisfied.
    pub(super) fn check_route(
        step: &crate::engine::types::StepDefinition,
        route: &str,
        ctx: &Context,
    ) -> bool {
        // Look for _route_{dependency_name} keys in context
        for dep in &step.dependencies {
            let route_key = format!("_route_{}", dep);
            if let Some(serde_json::Value::String(r)) = ctx.get(&route_key)
                && r == route
            {
                return true;
            }
        }
        false
    }
}
