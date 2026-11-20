use anyhow::Result;

use super::comments::{attach_comment_anchors, parse_docx_comments, validated_comment_ids};
use super::content::format_docx_content;
use super::metadata::extract_docx_metadata;
use super::package::{DocxPackage, Linked};
use super::stories::{Reader, StoryOptions};
use crate::artifacts::FileSource;
use crate::engine::types::NodeOutput;
use crate::nodes::extract::docx_parser::{
    Lookups, parse_docx_blocks, parse_numbering_defs, parse_styles, parse_theme_colors,
};
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::resource::{Budget, Limits};
use crate::util::execution::ExecutionControl;

pub(super) struct Request {
    pub(super) source: FileSource,
    pub(super) format: String,
    pub(super) output_key: String,
    pub(super) metadata_key: Option<String>,
    pub(super) comments_key: Option<String>,
    pub(super) stories: StoryOptions,
}

pub(super) fn extract(request: Request, execution: ExecutionControl) -> Result<NodeOutput> {
    let limits = Limits::current();
    let mut budget = Budget::new("extract_word", limits, &execution);
    let mut archive = Archive::open(&request.source, "extract_word", limits, &execution)?;

    let package = DocxPackage::open(&mut archive, &mut budget, &execution)?;
    let (numbering, theme, styles) =
        read_document_lookups(&mut archive, &package, &mut budget, &execution)?;
    // Resolve requested comment IDs first so document text and comment anchors
    // can be collected during one streamed pass over the main document.
    let mut comments = if request.comments_key.is_some() {
        let part = package.part(Linked::Comments, "word/comments.xml", &mut budget)?;
        archive.with_optional_xml(&part, &execution, |reader| {
            parse_docx_comments(reader, &mut budget)
        })?
    } else {
        None
    };
    let comment_ids = comments
        .as_deref()
        .map(|comments| validated_comment_ids(comments, &budget))
        .transpose()?;
    let document = archive.with_required_xml(&package.document, &execution, |reader| {
        parse_docx_blocks(
            reader,
            &numbering,
            &theme,
            &styles,
            comment_ids.as_ref(),
            request.stories.notes,
            &mut budget,
        )
    })?;
    drop(comment_ids);
    let anchors = document.anchors;
    let blocks = document.blocks;
    let lookups = Lookups {
        numbering_defs: &numbering,
        theme_colors: &theme,
        styles: &styles,
    };
    let stories = Reader {
        archive: &mut archive,
        package: &package,
        lookups: &lookups,
        budget: &mut budget,
        execution: &execution,
    }
    .read(&document.references, request.stories)?;
    if let Some(comments) = comments.as_mut() {
        attach_comment_anchors(comments, &anchors, &budget)?;
    }
    drop(anchors);
    let content = format_docx_content(&blocks, &stories, &request.format, &mut budget)?;
    drop((blocks, stories));

    let mut output = NodeOutput::new();
    output.insert(request.output_key, content);
    if let Some(key) = request.metadata_key {
        let part = &package.core_properties;
        let metadata = archive
            .with_optional_xml(part, &execution, |reader| {
                extract_docx_metadata(reader, part, &mut budget)
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

type DocumentLookups = (
    std::collections::HashMap<String, bool>,
    std::collections::HashMap<String, String>,
    crate::nodes::extract::docx_parser::StyleSheet,
);

/// Numbering, theme colors and styles, each from the part the main document
/// names, or from its conventional `word/...` path.
fn read_document_lookups(
    archive: &mut Archive,
    package: &DocxPackage,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<DocumentLookups> {
    let part = package.part(Linked::Numbering, "word/numbering.xml", budget)?;
    let numbering = archive
        .with_optional_xml(&part, execution, |reader| {
            parse_numbering_defs(reader, budget)
        })?
        .unwrap_or_default();
    let part = package.part(Linked::Theme, "word/theme/theme1.xml", budget)?;
    let theme = archive
        .with_optional_xml(&part, execution, |reader| {
            parse_theme_colors(reader, budget)
        })?
        .unwrap_or_default();
    let part = package.part(Linked::Styles, "word/styles.xml", budget)?;
    let styles = archive
        .with_optional_xml(&part, execution, |reader| parse_styles(reader, budget))?
        .unwrap_or_default();
    Ok((numbering, theme, styles))
}
