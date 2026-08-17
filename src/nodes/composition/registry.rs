use std::sync::Arc;

use crate::nodes::NodeRegistry;

use super::{ParallelSubworkflowsNode, RepeatSubworkflowNode, SubworkflowNode, ToolDispatchNode};

pub(crate) fn register_nested(registry: &mut NodeRegistry, base_registry: Arc<NodeRegistry>) {
    registry.register(Arc::new(SubworkflowNode {
        base_registry: base_registry.clone(),
    }));
    registry.register(Arc::new(ParallelSubworkflowsNode {
        base_registry: base_registry.clone(),
    }));
    registry.register(Arc::new(RepeatSubworkflowNode {
        base_registry: base_registry.clone(),
    }));
    registry.register(Arc::new(ToolDispatchNode { base_registry }));
}

pub(super) fn child_registry(base_registry: &Arc<NodeRegistry>) -> Arc<NodeRegistry> {
    let mut child = base_registry.snapshot();
    register_nested(&mut child, base_registry.clone());
    Arc::new(child)
}
