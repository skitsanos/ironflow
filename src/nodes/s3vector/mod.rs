mod bucket;
mod client;
mod config;
mod document;
mod index;
mod parameters;
mod vectors;
mod vectors_delete;
mod vectors_put;
mod vectors_query;

pub use bucket::{S3VectorCreateBucketNode, S3VectorGetBucketNode};
pub use index::{S3VectorCreateIndexNode, S3VectorGetIndexNode};
pub use vectors_delete::S3VectorDeleteVectorsNode;
pub use vectors_put::S3VectorPutVectorsNode;
pub use vectors_query::S3VectorQueryVectorsNode;

use std::sync::Arc;

pub fn register_all(registry: &mut crate::nodes::NodeRegistry) {
    registry.register(Arc::new(S3VectorCreateBucketNode));
    registry.register(Arc::new(S3VectorGetBucketNode));
    registry.register(Arc::new(S3VectorCreateIndexNode));
    registry.register(Arc::new(S3VectorGetIndexNode));
    registry.register(Arc::new(S3VectorPutVectorsNode));
    registry.register(Arc::new(S3VectorQueryVectorsNode));
    registry.register(Arc::new(S3VectorDeleteVectorsNode));
}
