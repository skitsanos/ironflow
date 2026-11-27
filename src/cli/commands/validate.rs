use std::path::Path;

use anyhow::{Context as _, Result};

use crate::cli::{AuthoringFormat, ValidateArgs};
use crate::engine::recovery::ExecutionPlan;
use crate::engine::types::FlowDefinition;
use crate::lua::LuaRuntime;
use crate::lua::runtime::ValidatedFlow;
use crate::nodes::NodeRegistry;

mod children;
mod context;
mod references;
mod report;

use report::{Diagnostic, print_json};

/// Unknown node types and execution-graph errors of one flow definition.
fn definition_errors(flow: &FlowDefinition, registry: &NodeRegistry) -> Vec<String> {
    let mut errors = unknown_nodes(flow, registry);
    errors.extend(flow.validate_dag());
    errors
}

fn unknown_nodes(flow: &FlowDefinition, registry: &NodeRegistry) -> Vec<String> {
    flow.steps
        .iter()
        .filter(|step| registry.get(&step.node_type).is_none())
        .map(|step| {
            format!(
                "Step '{}' uses unknown node type '{}'",
                step.name, step.node_type
            )
        })
        .collect()
}

pub(crate) fn cmd_validate(args: ValidateArgs) -> Result<()> {
    let ValidateArgs {
        flow: flow_path,
        strict,
        format,
        context_keys,
    } = args;
    let registry = NodeRegistry::with_builtins();

    let flow_str = flow_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid flow path"))?;

    let validated = match LuaRuntime::validate_flow(flow_str, &registry)
        .with_context(|| format!("Failed to load flow: {}", flow_path.display()))
    {
        Ok(validated) => validated,
        Err(error) => {
            if matches!(format, AuthoringFormat::Json) {
                print_json(
                    &flow_path,
                    false,
                    &[Diagnostic::new(
                        "flow_load_error",
                        "error",
                        format!("{error:#}"),
                    )],
                )?;
                anyhow::bail!("Flow validation failed");
            }
            return Err(error);
        }
    };
    let flow = &validated.flow;

    let mut errors: Vec<_> = unknown_nodes(flow, &registry)
        .into_iter()
        .map(|message| Diagnostic::new("unknown_node", "error", message))
        .collect();
    let context_warnings = match ExecutionPlan::build(flow) {
        Ok(plan) => context::warnings(flow, &plan, &context_keys),
        Err(graph_errors) => {
            errors.extend(
                graph_errors
                    .into_iter()
                    .map(|message| Diagnostic::new("invalid_graph", "error", message)),
            );
            Vec::new()
        }
    };
    // Child flows are loaded and checked too, so a broken or recursive child
    // is found here instead of by the first run (IF-220).
    let children = children::check(&flow_path, flow, &registry);
    errors.extend(
        children
            .errors
            .into_iter()
            .map(|message| Diagnostic::new("invalid_child_flow", "error", message)),
    );
    let warnings = validated.warnings.len() + children.warnings.len() + context_warnings.len();
    let strict_failure = strict && warnings > 0;
    let valid = errors.is_empty() && !strict_failure;
    if matches!(format, AuthoringFormat::Json) {
        let mut diagnostics = errors;
        diagnostics.extend(validated.warnings.into_iter().map(Diagnostic::from));
        diagnostics.extend(
            children
                .warnings
                .into_iter()
                .map(|message| Diagnostic::new("child_flow_warning", "warning", message)),
        );
        diagnostics.extend(context_warnings);
        print_json(&flow_path, valid, &diagnostics)?;
        anyhow::ensure!(valid, "Flow validation failed");
        return Ok(());
    }

    println!("Flow: {}", flow.name);
    println!("Steps: {}", flow.steps.len());
    if children.flows > 0 {
        println!("Child flows: {}", children.flows);
    }
    if warnings > 0 {
        print_warnings(
            &flow_path,
            &validated,
            &children.warnings,
            &context_warnings,
        );
    }
    if valid {
        println!("Validation: OK");
        print_execution_order(flow);
        return Ok(());
    }

    println!("Validation: FAILED");
    for err in &errors {
        println!("  - {}", err.message);
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

fn print_execution_order(flow: &FlowDefinition) {
    println!("\nExecution order:");
    for step in &flow.steps {
        let deps = if step.dependencies.is_empty() {
            String::from("(no dependencies)")
        } else {
            format!("depends on: {}", step.dependencies.join(", "))
        };
        println!("  {} [{}] {}", step.name, step.node_type, deps);
    }
}

fn print_warnings(
    flow_path: &Path,
    validated: &ValidatedFlow,
    children: &[String],
    context: &[Diagnostic],
) {
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
    for warning in context {
        println!(
            "  {}:step[{}] [{}] {}",
            flow_path.display(),
            warning.step.as_deref().unwrap_or_default(),
            warning.code,
            warning.message,
        );
    }
}
