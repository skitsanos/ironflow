pub mod ai;
mod child_process;
pub mod cloud;
pub mod composition;
pub mod database;
mod error;
pub mod extract;
pub mod file;
pub mod http;
pub mod image;
pub mod mcp;
pub mod notify;
pub mod s3vector;
pub mod transform;
pub mod utility;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};

pub use error::NodeFailure;

/// Trait that all nodes must implement.
#[async_trait]
pub trait Node: Send + Sync {
    /// Node type identifier (e.g., "http_get", "shell_command").
    fn node_type(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// Execute the node with the given configuration and phase-start context.
    ///
    /// Nodes receive `ctx` by shared reference and must not assume they can
    /// mutate it. Independent phase members and all their retries receive the
    /// same snapshot. The engine publishes successful `NodeOutput` values at
    /// the phase barrier in flow declaration order. A [`NodeFailure`] may carry
    /// output for terminal-only barrier publication after retries are
    /// exhausted. Taking `&Context` lets the executor share one `Arc<Context>`
    /// across parallel attempts instead of deep-cloning the whole map on every
    /// attempt.
    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput>;
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
        utility::register_all(&mut registry);
        ai::register_all(&mut registry);
        cloud::register_all(&mut registry);
        composition::register_all(&mut registry);
        extract::register_all(&mut registry);
        file::register_all(&mut registry);
        http::register_all(&mut registry);
        notify::register_all(&mut registry);
        database::register_all(&mut registry);
        image::register_all(&mut registry);
        mcp::register_all(&mut registry);
        s3vector::register_all(&mut registry);
        transform::register_all(&mut registry);

        // Registry-backed composition nodes receive a base snapshot and add
        // the complete composition set to every nested child engine.
        let base = Arc::new(registry.snapshot());
        composition::register_nested(&mut registry, base);

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
