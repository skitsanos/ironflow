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
        registry
    }

    /// Register a node implementation.
    pub fn register(&mut self, node: Arc<dyn Node>) {
        self.nodes.insert(node.node_type().to_string(), node);
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
