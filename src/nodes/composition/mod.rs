mod conditional;
mod foreach;
pub mod parallel_subworkflows;
pub mod subworkflow;
pub mod tool_dispatch;

pub use conditional::{IfBodyContainsNode, IfHttpStatusNode, IfNode, SwitchNode};
pub use foreach::ForEachNode;
pub use parallel_subworkflows::ParallelSubworkflowsNode;
pub use subworkflow::SubworkflowNode;
pub use tool_dispatch::ToolDispatchNode;

use crate::nodes::NodeRegistry;
use std::sync::Arc;

/// Register conditional and foreach nodes.
/// SubworkflowNode and ParallelSubworkflowsNode are constructed separately
/// in with_builtins (after the base snapshot) and must NOT be registered here.
pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(IfNode));
    registry.register(Arc::new(SwitchNode));
    registry.register(Arc::new(IfHttpStatusNode));
    registry.register(Arc::new(IfBodyContainsNode));
    registry.register(Arc::new(ForEachNode));
}
