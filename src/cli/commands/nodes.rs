use anyhow::Result;

use crate::cli::{AuthoringFormat, NodesArgs, NodesCommand};
use crate::nodes::NodeRegistry;
use crate::nodes::http::output::{DEFAULT_PREFIX, ResponseMode};

pub(crate) fn cmd_nodes(args: NodesArgs) -> Result<()> {
    let registry = NodeRegistry::with_builtins();
    if let Some(NodesCommand::Describe { node, format }) = args.command {
        let registered = registry
            .get(&node)
            .ok_or_else(|| anyhow::anyhow!("Unknown node '{node}'"))?;
        anyhow::ensure!(
            node == "http_get",
            "No output contract is available for '{node}'"
        );
        return describe_http_get(registered.description(), format);
    }
    let nodes = registry.list();

    println!("{:<20} DESCRIPTION", "NODE TYPE");
    println!("{}", "-".repeat(60));

    for (name, desc) in &nodes {
        println!("{:<20} {}", name, desc);
    }

    println!("\nTotal: {} node(s)", nodes.len());
    Ok(())
}

fn describe_http_get(description: &str, format: AuthoringFormat) -> Result<()> {
    let outputs = |mode: ResponseMode| mode.fields().map(|field| field.key("{output_key}"));
    let inline = outputs(ResponseMode::Inline);
    let artifact = outputs(ResponseMode::Artifact);
    match format {
        AuthoringFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1,
                "ironflow_version": env!("CARGO_PKG_VERSION"),
                "node": "http_get",
                "description": description,
                "defaults": {"output_key": DEFAULT_PREFIX, "response_mode": "inline"},
                "outputs_on_success": {"inline": inline, "artifact": artifact},
                "example": include_str!("../../nodes/http/http_get_context.lua"),
            }))?
        ),
        AuthoringFormat::Text => {
            println!("http_get: {description}");
            println!("Default output_key: {DEFAULT_PREFIX}; default response_mode: inline");
            println!("Outputs on success (inline): {}", inline.join(", "));
            println!("Outputs on success (artifact): {}", artifact.join(", "));
        }
    }
    Ok(())
}
