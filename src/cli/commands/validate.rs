use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

use crate::engine::types::FlowDefinition;
use crate::lua::LuaRuntime;
use crate::lua::runtime::ValidatedFlow;
use crate::nodes::NodeRegistry;

mod children;
mod references;

/// Unknown node types and execution-graph errors of one flow definition.
fn definition_errors(flow: &FlowDefinition, registry: &NodeRegistry) -> Vec<String> {
    let mut errors: Vec<String> = flow
        .steps
        .iter()
        .filter(|step| registry.get(&step.node_type).is_none())
        .map(|step| {
            format!(
                "Step '{}' uses unknown node type '{}'",
                step.name, step.node_type
            )
        })
        .collect();
    // Dependencies, failure-recovery edges and cycle detection.
    errors.extend(flow.validate_dag());
    errors
}

pub(crate) fn cmd_validate(flow_path: PathBuf, strict: bool) -> Result<()> {
    let registry = NodeRegistry::with_builtins();

    let flow_str = flow_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid flow path"))?;

    let validated = LuaRuntime::validate_flow(flow_str, &registry)
        .with_context(|| format!("Failed to load flow: {}", flow_path.display()))?;
    let flow = &validated.flow;

    println!("Flow: {}", flow.name);
    println!("Steps: {}", flow.steps.len());

    let mut errors = definition_errors(flow, &registry);
    // Child flows are loaded and checked too, so a broken or recursive child
    // is found here instead of by the first run (IF-220).
    let children = children::check(&flow_path, flow, &registry);
    if children.flows > 0 {
        println!("Child flows: {}", children.flows);
    }
    errors.extend(children.errors);

    let warnings = print_warnings(&flow_path, &validated, &children.warnings);
    let strict_failure = strict && warnings > 0;
    if errors.is_empty() && !strict_failure {
        println!("Validation: OK");

        println!("\nExecution order:");
        for step in &flow.steps {
            let deps = if step.dependencies.is_empty() {
                String::from("(no dependencies)")
            } else {
                format!("depends on: {}", step.dependencies.join(", "))
            };
            println!("  {} [{}] {}", step.name, step.node_type, deps);
        }
        return Ok(());
    }

    println!("Validation: FAILED");
    for err in &errors {
        println!("  - {}", err);
    }
    if strict_failure {
        println!("  - strict validation rejected {warnings} warning(s)");
    }
    anyhow::bail!(
        "{} validation error(s) and {} strict warning(s) found",
        errors.len(),
        usize::from(strict_failure) * warnings
    );
}

/// Print Lua handler diagnostics and child-flow warnings; returns their count.
fn print_warnings(flow_path: &Path, validated: &ValidatedFlow, children: &[String]) -> usize {
    let count = validated.warnings.len() + children.len();
    if count == 0 {
        return 0;
    }
    println!("Warnings:");
    for warning in &validated.warnings {
        if let Some(step) = &warning.step {
            println!(
                "  {}:step[{}].source:{}:{} [{}] {}",
                flow_path.display(),
                step,
                warning.line,
                warning.column,
                warning.code,
                warning.message
            );
        } else {
            println!(
                "  {}:{}:{} [{}] {}",
                flow_path.display(),
                warning.line,
                warning.column,
                warning.code,
                warning.message
            );
        }
    }
    for warning in children {
        println!("  {warning}");
    }
    count
}
