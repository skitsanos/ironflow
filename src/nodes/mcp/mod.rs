mod client;
mod protocol;
mod session;
mod transport;

pub use client::McpClientNode;

use crate::nodes::NodeRegistry;
use std::sync::Arc;

pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(McpClientNode));
}
