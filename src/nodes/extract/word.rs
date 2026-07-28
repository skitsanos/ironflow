mod comments;
mod content;
mod metadata;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::common::validate_word_format;
use crate::util::node_config::get_path;
use comments::extract_docx_comments;
use content::extract_docx_content;
use metadata::extract_docx_metadata;

pub struct ExtractWordNode;

#[async_trait]
impl Node for ExtractWordNode {
    fn node_type(&self) -> &str {
        "extract_word"
    }

    fn description(&self) -> &str {
        "Extract text and metadata from a Word (.docx) document"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = get_path(config, ctx, "extract_word")?;
        let format = validate_word_format(config, "extract_word")?;
        let output_key = config
            .get("output_key")
            .and_then(|value| value.as_str())
            .unwrap_or("content");
        let metadata_key = config.get("metadata_key").and_then(|value| value.as_str());
        let comments_key = config.get("comments_key").and_then(|value| value.as_str());

        let file = std::fs::File::open(&path)
            .map_err(|error| anyhow::anyhow!("Failed to open '{}': {}", path, error))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|error| {
            anyhow::anyhow!("Failed to read DOCX archive '{}': {}", path, error)
        })?;
        let content = extract_docx_content(&mut archive, format)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), content);
        if let Some(key) = metadata_key {
            output.insert(
                key.to_string(),
                serde_json::to_value(extract_docx_metadata(&mut archive))?,
            );
        }
        if let Some(key) = comments_key {
            output.insert(
                key.to_string(),
                serde_json::to_value(extract_docx_comments(&mut archive))?,
            );
        }
        Ok(output)
    }
}
