use std::path::PathBuf;

use anyhow::{Context as _, Result};

use crate::lua::LuaRuntime;
use crate::nodes::NodeRegistry;

pub(crate) fn cmd_validate(flow_path: PathBuf, strict: bool) -> Result<()> {
    let registry = NodeRegistry::with_builtins();

    let flow_str = flow_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid flow path"))?;

    let validated = LuaRuntime::validate_flow(flow_str, &registry)
        .with_context(|| format!("Failed to load flow: {}", flow_path.display()))?;
    let flow = validated.flow;

    println!("Flow: {}", flow.name);
    println!("Steps: {}", flow.steps.len());

    // Validate all node types exist
    let mut errors = Vec::new();
    for step in &flow.steps {
        if registry.get(&step.node_type).is_none() {
            errors.push(format!(
                "Step '{}' uses unknown node type '{}'",
                step.name, step.node_type
            ));
        }
    }

    // Validate DAG (dependencies + cycle detection)
    errors.extend(flow.validate_dag());

    if !validated.warnings.is_empty() {
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
    }

    let strict_failure = strict && !validated.warnings.is_empty();
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
    } else {
        println!("Validation: FAILED");
        for err in &errors {
            println!("  - {}", err);
        }
        if strict_failure {
            println!(
                "  - strict validation rejected {} warning(s)",
                validated.warnings.len()
            );
        }
        anyhow::bail!(
            "{} validation error(s) and {} strict warning(s) found",
            errors.len(),
            usize::from(strict_failure) * validated.warnings.len()
        );
    }

    Ok(())
}
