use std::path::PathBuf;

use anyhow::{Context as _, Result};

use crate::lua::LuaRuntime;
use crate::nodes::NodeRegistry;

pub(crate) fn cmd_validate(flow_path: PathBuf) -> Result<()> {
    let registry = NodeRegistry::with_builtins();

    let flow_str = flow_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid flow path"))?;

    let flow = LuaRuntime::load_flow(flow_str, &registry)
        .with_context(|| format!("Failed to load flow: {}", flow_path.display()))?;

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

    if errors.is_empty() {
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
        anyhow::bail!("{} validation error(s) found", errors.len());
    }

    Ok(())
}
