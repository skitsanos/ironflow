mod arangodb;
mod sql;

pub use arangodb::ArangoDbAqlNode;
pub use sql::{DbExecNode, DbQueryNode};

use crate::nodes::NodeRegistry;
use std::sync::Arc;

pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(DbQueryNode));
    registry.register(Arc::new(DbExecNode));
    registry.register(Arc::new(ArangoDbAqlNode));
}
