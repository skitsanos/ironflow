mod comments;
mod content;
mod metadata;
mod worker;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::common::{ensure_distinct_keys, optional_string, string_or, validate_word_format};
use crate::util::file_source::get_file_source;

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
        let source = get_file_source(config, ctx, "extract_word")?;
        let format = validate_word_format(config, "extract_word")?.to_string();
        let output_key = string_or(config, "output_key", "content", "extract_word")?;
        let metadata_key = optional_string(config, "metadata_key", "extract_word")?;
        let comments_key = optional_string(config, "comments_key", "extract_word")?;
        let mut keys = vec![("output_key", output_key)];
        if let Some(key) = metadata_key {
            keys.push(("metadata_key", key));
        }
        if let Some(key) = comments_key {
            keys.push(("comments_key", key));
        }
        ensure_distinct_keys("extract_word", &keys)?;
        let request = worker::Request {
            source,
            format,
            output_key: output_key.to_string(),
            metadata_key: metadata_key.map(str::to_string),
            comments_key: comments_key.map(str::to_string),
        };

        crate::util::execution::run_tracked_blocking_step(move |execution| {
            worker::extract(request, execution)
        })
        .await
    }
}
