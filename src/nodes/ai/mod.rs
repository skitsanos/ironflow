mod embeddings;

mod chunking;
mod chunking_merge;
mod chunking_semantic;
mod chunking_semantic_engine;
mod llm;
mod llm_providers;
pub(crate) mod llm_response;

pub use chunking::AiChunkNode;
pub use chunking_merge::AiChunkMergeNode;
pub use chunking_semantic::AiChunkSemanticNode;
pub use embeddings::AiEmbedNode;
pub use llm::LlmNode;

use crate::nodes::NodeRegistry;
use std::sync::Arc;

pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(AiEmbedNode));
    registry.register(Arc::new(AiChunkNode));
    registry.register(Arc::new(AiChunkMergeNode));
    registry.register(Arc::new(AiChunkSemanticNode));
    registry.register(Arc::new(LlmNode));
}
