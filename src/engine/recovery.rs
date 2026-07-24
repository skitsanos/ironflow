use std::collections::{BTreeSet, HashMap, HashSet};

use super::types::FlowDefinition;

/// Validated execution graph, including failure-triggered recovery work.
#[derive(Debug, Clone)]
pub(crate) struct ExecutionPlan {
    /// Topological phases whose members may execute concurrently.
    pub(crate) phases: Vec<Vec<String>>,
    /// Recovery handler name to the source step whose failure activates it.
    pub(crate) recovery_sources: HashMap<String, String>,
}

impl ExecutionPlan {
    /// Validate a flow and construct its complete execution graph.
    pub(crate) fn build(flow: &FlowDefinition) -> Result<Self, Vec<String>> {
        let mut errors = validate_step_options(flow);
        let (steps_by_name, duplicate_errors) = index_steps(flow);
        errors.extend(duplicate_errors);
        errors.extend(validate_dependencies(flow, &steps_by_name));

        let (recovery_sources, recovery_errors) = validate_recovery_handlers(flow, &steps_by_name);
        errors.extend(recovery_errors);

        if !errors.is_empty() {
            return Err(errors);
        }

        let graph = build_graph(flow, &recovery_sources);
        let declaration_order: HashMap<String, usize> = flow
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| (step.name.clone(), index))
            .collect();
        let phases = topological_phases(graph, &declaration_order)?;

        Ok(Self {
            phases,
            recovery_sources,
        })
    }
}

fn validate_step_options(flow: &FlowDefinition) -> Vec<String> {
    let mut errors = Vec::new();

    for step in &flow.steps {
        for (path, error) in crate::lua::interpolate::validate_value(&step.config) {
            errors.push(format!("Step '{}' {path}: {error}", step.name));
        }

        if step.retry.max_retries == u32::MAX {
            errors.push(format!("Step '{}' retry count is too large", step.name));
        }
        if !step.retry.backoff_s.is_finite() || step.retry.backoff_s < 0.0 {
            errors.push(format!(
                "Step '{}' retry backoff must be a finite, non-negative number of seconds",
                step.name
            ));
        }
        if let Some(timeout) = step.timeout_s
            && (!timeout.is_finite() || timeout <= 0.0)
        {
            errors.push(format!(
                "Step '{}' timeout must be a finite number greater than zero",
                step.name
            ));
        }
    }

    errors
}

fn index_steps(
    flow: &FlowDefinition,
) -> (HashMap<&str, &super::types::StepDefinition>, Vec<String>) {
    let mut steps = HashMap::new();
    let mut errors = Vec::new();

    for step in &flow.steps {
        if steps.insert(step.name.as_str(), step).is_some() {
            errors.push(format!("Duplicate step name '{}'", step.name));
        }
    }

    (steps, errors)
}

fn validate_dependencies(
    flow: &FlowDefinition,
    steps_by_name: &HashMap<&str, &super::types::StepDefinition>,
) -> Vec<String> {
    let mut errors = Vec::new();

    for step in &flow.steps {
        for dependency in &step.dependencies {
            if !steps_by_name.contains_key(dependency.as_str()) {
                errors.push(format!(
                    "Step '{}' depends on '{}', which does not exist",
                    step.name, dependency
                ));
            }
        }
    }

    errors
}

fn validate_recovery_handlers(
    flow: &FlowDefinition,
    steps_by_name: &HashMap<&str, &super::types::StepDefinition>,
) -> (HashMap<String, String>, Vec<String>) {
    let mut recovery_sources = HashMap::new();
    let mut errors = Vec::new();

    for source in &flow.steps {
        let Some(handler_name) = source.on_error.as_deref() else {
            continue;
        };

        let Some(handler) = steps_by_name.get(handler_name) else {
            errors.push(format!(
                "Step '{}' on_error target '{}' does not exist",
                source.name, handler_name
            ));
            continue;
        };

        if source.name == handler_name {
            errors.push(format!(
                "Step '{}' cannot use itself as an on_error handler",
                source.name
            ));
            continue;
        }

        if let Some(existing_source) =
            recovery_sources.insert(handler_name.to_string(), source.name.clone())
        {
            errors.push(format!(
                "Recovery handler '{}' is assigned to multiple source steps: '{}' and '{}'",
                handler_name, existing_source, source.name
            ));
            continue;
        }

        if handler.on_error.is_some() {
            errors.push(format!(
                "Recovery handler '{}' cannot declare on_error",
                handler_name
            ));
        }
        if handler.route.is_some() {
            errors.push(format!(
                "Recovery handler '{}' cannot declare a route",
                handler_name
            ));
        }
        if handler.dependencies.contains(&source.name) {
            errors.push(format!(
                "Recovery handler '{}' cannot depend on its owning source step '{}'",
                handler_name, source.name
            ));
        }
    }

    (recovery_sources, errors)
}

fn build_graph(
    flow: &FlowDefinition,
    recovery_sources: &HashMap<String, String>,
) -> HashMap<String, BTreeSet<String>> {
    let mut graph: HashMap<String, BTreeSet<String>> = flow
        .steps
        .iter()
        .map(|step| (step.name.clone(), BTreeSet::new()))
        .collect();

    for step in &flow.steps {
        for dependency in &step.dependencies {
            add_edge(&mut graph, dependency, &step.name);
        }
    }

    for (handler, source) in recovery_sources {
        add_edge(&mut graph, source, handler);

        for dependent in &flow.steps {
            if dependent.name != *handler && dependent.dependencies.contains(source) {
                add_edge(&mut graph, handler, &dependent.name);
            }
        }
    }

    graph
}

fn add_edge(graph: &mut HashMap<String, BTreeSet<String>>, from: &str, to: &str) {
    graph
        .get_mut(from)
        .expect("validated graph node must exist")
        .insert(to.to_string());
}

fn topological_phases(
    graph: HashMap<String, BTreeSet<String>>,
    declaration_order: &HashMap<String, usize>,
) -> Result<Vec<Vec<String>>, Vec<String>> {
    let mut in_degree: HashMap<String, usize> =
        graph.keys().map(|name| (name.clone(), 0)).collect();
    for dependents in graph.values() {
        for dependent in dependents {
            *in_degree
                .get_mut(dependent)
                .expect("validated graph node must exist") += 1;
        }
    }

    let mut remaining: HashSet<String> = graph.keys().cloned().collect();
    let mut phases = Vec::new();

    while !remaining.is_empty() {
        let mut ready: Vec<String> = remaining
            .iter()
            .filter(|name| in_degree.get(*name).copied().unwrap_or(0) == 0)
            .cloned()
            .collect();
        ready.sort_by_key(|name| declaration_order[name]);

        if ready.is_empty() {
            let mut cycle_steps: Vec<String> = remaining.into_iter().collect();
            cycle_steps.sort_by_key(|name| declaration_order[name]);
            return Err(vec![format!(
                "Cycle detected in flow DAG involving steps: {}",
                cycle_steps.join(", ")
            )]);
        }

        for name in &ready {
            remaining.remove(name);
            for dependent in &graph[name] {
                *in_degree
                    .get_mut(dependent)
                    .expect("validated graph node must exist") -= 1;
            }
        }
        phases.push(ready);
    }

    Ok(phases)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::engine::types::{RetryConfig, StepDefinition};

    fn step(name: &str, dependencies: &[&str]) -> StepDefinition {
        StepDefinition {
            name: name.to_string(),
            node_type: "code".to_string(),
            config: json!({}),
            dependencies: dependencies
                .iter()
                .map(|dependency| (*dependency).to_string())
                .collect(),
            retry: RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        }
    }

    #[test]
    fn phase_members_follow_flow_declaration_order() {
        let flow = FlowDefinition {
            name: "declaration_order".to_string(),
            steps: vec![
                step("z_first", &[]),
                step("a_second", &[]),
                step("m_after", &["z_first", "a_second"]),
            ],
        };

        let plan = ExecutionPlan::build(&flow).unwrap();

        assert_eq!(
            plan.phases,
            vec![
                vec!["z_first".to_string(), "a_second".to_string()],
                vec!["m_after".to_string()],
            ]
        );
    }
}
