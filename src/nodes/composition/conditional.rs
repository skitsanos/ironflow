mod basic;
mod body;
mod comparison;
mod expression;
mod http;

pub use basic::{IfNode, SwitchNode};
pub use body::IfBodyContainsNode;
pub use http::IfHttpStatusNode;

use crate::engine::types::FlowDefinition;

/// Reject `if_node` conditions that are invalid whatever the context holds,
/// so a flow fails to load instead of failing or skipping a step at runtime.
pub(crate) fn validate_guard_conditions(flow: &FlowDefinition) -> anyhow::Result<()> {
    for step in flow.steps.iter().filter(|step| step.node_type == "if_node") {
        let Some(condition) = step
            .config
            .get("condition")
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        expression::validate_condition(condition).map_err(|error| {
            anyhow::anyhow!(
                "Step '{}' has an invalid condition '{condition}': {error}",
                step.name
            )
        })?;
    }
    Ok(())
}
