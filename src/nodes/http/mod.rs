mod helpers;
mod request;

pub use request::{HttpDeleteNode, HttpGetNode, HttpPostNode, HttpPutNode, HttpRequestNode};

use crate::nodes::NodeRegistry;
use std::sync::Arc;

pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(HttpRequestNode));
    registry.register(Arc::new(HttpGetNode));
    registry.register(Arc::new(HttpPostNode));
    registry.register(Arc::new(HttpPutNode));
    registry.register(Arc::new(HttpDeleteNode));
}
