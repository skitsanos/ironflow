use anyhow::Result;

use crate::nodes::NodeRegistry;

pub(crate) fn cmd_nodes() -> Result<()> {
    let registry = NodeRegistry::with_builtins();
    let nodes = registry.list();

    println!("{:<20} DESCRIPTION", "NODE TYPE");
    println!("{}", "-".repeat(60));

    for (name, desc) in &nodes {
        println!("{:<20} {}", name, desc);
    }

    println!("\nTotal: {} node(s)", nodes.len());
    Ok(())
}
