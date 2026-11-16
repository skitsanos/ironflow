mod basic;
mod body;
mod comparison;
mod expression;
mod http;

pub use basic::{IfNode, SwitchNode};
pub use body::IfBodyContainsNode;
pub use http::IfHttpStatusNode;

use crate::engine::types::FlowDefinition;

/// `if_node` conditions that are invalid whatever the context holds. The
/// execution plan reports them for every flow definition, including one built
/// without the Lua loader (IF-220).
pub(crate) fn guard_condition_errors(flow: &FlowDefinition) -> Vec<String> {
    flow.steps
        .iter()
        .filter(|step| step.node_type == "if_node")
        .filter_map(|step| {
            let condition = step.config.get("condition")?.as_str()?;
            let error = expression::validate_condition(condition).err()?;
            Some(format!(
                "Step '{}' has an invalid condition '{condition}': {error}",
                step.name
            ))
        })
        .collect()
}

/// Reject such conditions when a flow is loaded, so it fails to load instead
/// of failing or skipping a step at runtime.
pub(crate) fn validate_guard_conditions(flow: &FlowDefinition) -> anyhow::Result<()> {
    match guard_condition_errors(flow).into_iter().next() {
        Some(error) => Err(anyhow::anyhow!(error)),
        None => Ok(()),
    }
}
