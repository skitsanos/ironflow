//! The flow check every run-starting handler applies before a run is
//! created: the same validation `POST /flows/validate` reports.

use crate::engine::types::FlowDefinition;
use crate::nodes::NodeRegistry;

/// Every reason the engine would refuse to start `flow`: node types the
/// registry does not know, then the graph and step-option errors.
pub(super) fn flow_validation_errors(
    flow: &FlowDefinition,
    registry: &NodeRegistry,
) -> Vec<String> {
    let mut errors = flow
        .steps
        .iter()
        .filter(|step| registry.get(&step.node_type).is_none())
        .map(|step| {
            format!(
                "Step '{}' uses unknown node type '{}'",
                step.name, step.node_type
            )
        })
        .collect::<Vec<_>>();
    errors.extend(flow.validate_dag());
    errors
}
