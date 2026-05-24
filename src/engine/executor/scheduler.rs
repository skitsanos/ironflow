use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};

use crate::engine::types::{Context, FlowDefinition};

use super::engine::WorkflowEngine;

impl WorkflowEngine {
    /// Topological sort using Kahn's algorithm. Returns execution phases.
    /// Each phase is a vec of step names that can run in parallel.
    pub(super) fn topological_sort(&self, flow: &FlowDefinition) -> Result<Vec<Vec<String>>> {
        let step_names: HashSet<String> = flow.steps.iter().map(|s| s.name.clone()).collect();

        // Validate all dependencies exist
        for step in &flow.steps {
            for dep in &step.dependencies {
                if !step_names.contains(dep) {
                    bail!(
                        "Step '{}' depends on '{}', which does not exist",
                        step.name,
                        dep
                    );
                }
            }
        }

        // Build adjacency and in-degree maps
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for step in &flow.steps {
            in_degree.entry(step.name.clone()).or_insert(0);
            for dep in &step.dependencies {
                dependents
                    .entry(dep.clone())
                    .or_default()
                    .push(step.name.clone());
                *in_degree.entry(step.name.clone()).or_insert(0) += 1;
            }
        }

        let mut phases: Vec<Vec<String>> = Vec::new();
        let mut remaining: HashSet<String> = step_names;

        loop {
            // Find all nodes with in-degree 0 that are still remaining
            let ready: Vec<String> = remaining
                .iter()
                .filter(|name| in_degree.get(*name).copied().unwrap_or(0) == 0)
                .cloned()
                .collect();

            if ready.is_empty() {
                if remaining.is_empty() {
                    break;
                } else {
                    bail!(
                        "Cycle detected in flow DAG. Remaining steps: {:?}",
                        remaining
                    );
                }
            }

            // Remove ready nodes and reduce in-degree of dependents
            for name in &ready {
                remaining.remove(name);
                if let Some(deps) = dependents.get(name) {
                    for dep in deps {
                        if let Some(deg) = in_degree.get_mut(dep) {
                            *deg -= 1;
                        }
                    }
                }
            }

            phases.push(ready);
        }

        Ok(phases)
    }

    /// Check if a step's route condition is satisfied.
    pub(super) fn check_route(
        &self,
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
