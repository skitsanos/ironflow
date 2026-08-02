use anyhow::Result;

use crate::nodes::extract::docx_parser::DocxBlock;
use crate::nodes::extract::resource::Budget;
use crate::nodes::extract::word_format::{blocks_to_json, blocks_to_markdown, blocks_to_text};

pub(super) fn format_docx_content(
    blocks: &[DocxBlock],
    format: &str,
    budget: &mut Budget<'_>,
) -> Result<serde_json::Value> {
    match format {
        "markdown" => Ok(serde_json::Value::String(blocks_to_markdown(
            blocks, budget,
        )?)),
        "json" => blocks_to_json(blocks, budget),
        _ => Ok(serde_json::Value::String(blocks_to_text(blocks, budget)?)),
    }
}
