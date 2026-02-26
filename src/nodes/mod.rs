pub mod builtin;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};

/// Trait that all nodes must implement.
#[async_trait]
pub trait Node: Send + Sync {
    /// Node type identifier (e.g., "http_get", "shell_command").
    fn node_type(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// Execute the node with the given configuration and current context.
    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput>;
}

/// Registry of available node types.
pub struct NodeRegistry {
    nodes: HashMap<String, Arc<dyn Node>>,
}

impl Default for NodeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    /// Create a registry with all built-in nodes registered.
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        builtin::register_all(&mut registry);

        // Snapshot the base registry (all nodes except subworkflow) and give
        // it to SubworkflowNode. It adds itself back at execution time so
        // child engines can also run subworkflows (nested execution).
        let base = Arc::new(registry.snapshot());
        registry.register(Arc::new(builtin::subworkflow_node::SubworkflowNode {
            base_registry: base,
        }));

        registry
    }

    /// Register a node implementation.
    pub fn register(&mut self, node: Arc<dyn Node>) {
        self.nodes.insert(node.node_type().to_string(), node);
    }

    /// Create a clone of this registry (all nodes are Arc-shared).
    pub fn snapshot(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
        }
    }

    /// Look up a node by type name.
    pub fn get(&self, node_type: &str) -> Option<Arc<dyn Node>> {
        self.nodes.get(node_type).cloned()
    }

    /// List all registered node types with descriptions.
    pub fn list(&self) -> Vec<(&str, &str)> {
        let mut entries: Vec<(&str, &str)> = self
            .nodes
            .values()
            .map(|n| (n.node_type(), n.description()))
            .collect();
        entries.sort_by_key(|(name, _)| *name);
        entries
    }
}
