use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub(crate) struct ExtractSrtNode;

#[async_trait]
impl Node for ExtractSrtNode {
    fn node_type(&self) -> &str {
        "extract_srt"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from SRT subtitle files"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        super::parser::extract(config, ctx, "extract_srt", "srt", false).await
    }
}
