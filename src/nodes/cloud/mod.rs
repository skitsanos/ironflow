mod s3_helpers;
mod s3_listing;
mod s3_objects;
mod s3_presign;

pub use s3_listing::{S3ListBucketsNode, S3ListObjectsNode};
pub use s3_objects::{S3CopyObjectNode, S3DeleteObjectNode, S3GetObjectNode, S3PutObjectNode};
pub use s3_presign::S3PresignUrlNode;

use crate::nodes::NodeRegistry;
use std::sync::Arc;

pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(S3PresignUrlNode));
    registry.register(Arc::new(S3GetObjectNode));
    registry.register(Arc::new(S3PutObjectNode));
    registry.register(Arc::new(S3DeleteObjectNode));
    registry.register(Arc::new(S3CopyObjectNode));
    registry.register(Arc::new(S3ListObjectsNode));
    registry.register(Arc::new(S3ListBucketsNode));
}
