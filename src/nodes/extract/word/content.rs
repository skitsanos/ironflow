use std::io::Read;

use anyhow::Result;

use crate::nodes::extract::docx_parser::{
    parse_docx_blocks, parse_numbering_defs, parse_theme_colors,
};
use crate::nodes::extract::word_format::{blocks_to_json, blocks_to_markdown, blocks_to_text};

pub(super) fn extract_docx_content(
    archive: &mut zip::ZipArchive<std::fs::File>,
    format: &str,
) -> Result<serde_json::Value> {
    let xml = {
        let mut entry = archive
            .by_name("word/document.xml")
            .map_err(|error| anyhow::anyhow!("Missing word/document.xml: {}", error))?;
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        xml
    };
    let numbering = parse_numbering_defs(archive);
    let theme_colors = parse_theme_colors(archive);
    let blocks = parse_docx_blocks(&xml, &numbering, &theme_colors);

    match format {
        "markdown" => Ok(serde_json::Value::String(blocks_to_markdown(&blocks))),
        "json" => Ok(blocks_to_json(&blocks)),
        _ => Ok(serde_json::Value::String(blocks_to_text(&blocks))),
    }
}
