mod archive;
mod directory;
mod helpers;
mod io;

pub use archive::{ZipCreateNode, ZipExtractNode, ZipListNode};
pub use directory::ListDirectoryNode;
pub use io::{CopyFileNode, DeleteFileNode, MoveFileNode, ReadFileNode, WriteFileNode};

use crate::nodes::NodeRegistry;
use std::sync::Arc;

pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(ReadFileNode));
    registry.register(Arc::new(WriteFileNode));
    registry.register(Arc::new(CopyFileNode));
    registry.register(Arc::new(MoveFileNode));
    registry.register(Arc::new(DeleteFileNode));
    registry.register(Arc::new(ListDirectoryNode));
    registry.register(Arc::new(ZipCreateNode));
    registry.register(Arc::new(ZipListNode));
    registry.register(Arc::new(ZipExtractNode));
}
