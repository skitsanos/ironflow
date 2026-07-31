use std::path::PathBuf;

use anyhow::Result;

use super::comments::{attach_comment_anchors, parse_docx_comments, validated_comment_ids};
use super::content::format_docx_content;
use super::metadata::extract_docx_metadata;
use crate::engine::types::NodeOutput;
use crate::nodes::extract::docx_parser::{
    parse_docx_blocks, parse_numbering_defs, parse_theme_colors,
};
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::resource::{Budget, Limits};
use crate::util::execution::ExecutionControl;

pub(super) struct Request {
    pub(super) path: PathBuf,
    pub(super) format: String,
    pub(super) output_key: String,
    pub(super) metadata_key: Option<String>,
    pub(super) comments_key: Option<String>,
}

pub(super) fn extract(request: Request, execution: ExecutionControl) -> Result<NodeOutput> {
    let limits = Limits::current();
    let mut budget = Budget::new("extract_word", limits, &execution);
    let mut archive = Archive::open(&request.path, "extract_word", limits, &execution)?;

    let numbering = archive
        .with_optional_xml("word/numbering.xml", &execution, |reader| {
            parse_numbering_defs(reader, &mut budget)
        })?
        .unwrap_or_default();
    let theme = archive
        .with_optional_xml("word/theme/theme1.xml", &execution, |reader| {
            parse_theme_colors(reader, &mut budget)
        })?
        .unwrap_or_default();
    // Resolve requested comment IDs first so document text and comment anchors
    // can be collected during one streamed pass over word/document.xml.
    let mut comments = if request.comments_key.is_some() {
        archive.with_optional_xml("word/comments.xml", &execution, |reader| {
            parse_docx_comments(reader, &mut budget)
        })?
    } else {
        None
    };
    let comment_ids = comments
        .as_deref()
        .map(|comments| validated_comment_ids(comments, &budget))
        .transpose()?;
    let document = archive.with_required_xml("word/document.xml", &execution, |reader| {
        parse_docx_blocks(
            reader,
            &numbering,
            &theme,
            comment_ids.as_ref(),
            &mut budget,
        )
    })?;
    drop(comment_ids);
    let anchors = document.anchors;
    let blocks = document.blocks;
    if let Some(comments) = comments.as_mut() {
        attach_comment_anchors(comments, &anchors, &budget)?;
    }
    drop(anchors);
    let content = format_docx_content(&blocks, &request.format, &mut budget)?;
    drop(blocks);

    let mut output = NodeOutput::new();
    output.insert(request.output_key, content);
    if let Some(key) = request.metadata_key {
        let metadata = archive
            .with_optional_xml("docProps/core.xml", &execution, |reader| {
                extract_docx_metadata(reader, &mut budget)
            })?
            .unwrap_or_default();
        output.insert(key, serde_json::to_value(metadata)?);
    }
    if let Some(key) = request.comments_key {
        output.insert(key, serde_json::to_value(comments.unwrap_or_default())?);
    }

    budget.ensure_output(&output)?;
    Ok(output)
}
