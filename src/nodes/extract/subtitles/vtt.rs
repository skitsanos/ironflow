use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub(crate) struct ExtractVttNode;

#[async_trait]
impl Node for ExtractVttNode {
    fn node_type(&self) -> &str {
        "extract_vtt"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from WebVTT subtitle files"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        super::parser::extract(config, ctx, "extract_vtt", "vtt", true)
    }
}
