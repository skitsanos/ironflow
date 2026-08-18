mod conditional;
mod foreach;
mod parallel_runner;
pub mod parallel_subworkflows;
mod registry;
pub mod repeat_subworkflow;
pub mod subworkflow;
pub mod tool_dispatch;

pub use conditional::{IfBodyContainsNode, IfHttpStatusNode, IfNode, SwitchNode};
pub use foreach::ForEachNode;
pub use parallel_subworkflows::ParallelSubworkflowsNode;
pub(crate) use registry::register_nested;
pub use repeat_subworkflow::RepeatSubworkflowNode;
pub use subworkflow::SubworkflowNode;
pub use tool_dispatch::ToolDispatchNode;

use crate::nodes::NodeRegistry;
use std::sync::Arc;

/// Register conditional and foreach nodes.
/// Registry-backed composition nodes are constructed separately in
/// `NodeRegistry::with_builtins` after the base snapshot.
pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(IfNode));
    registry.register(Arc::new(SwitchNode));
    registry.register(Arc::new(IfHttpStatusNode));
    registry.register(Arc::new(IfBodyContainsNode));
    registry.register(Arc::new(ForEachNode));
}
