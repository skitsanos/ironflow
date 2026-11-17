mod output;
mod stories;

use std::collections::HashMap;

use anyhow::Result;

use self::output::TextOutput;
use super::super::docx_parser::{DocxBlock, DocxParagraph, DocxStories, Flags};
use super::super::resource::Budget;

/// Plain text: headers (when requested), the body, the note definitions,
/// then footers, each section after a blank line.
pub(in crate::nodes::extract) fn blocks_to_text(
    blocks: &[DocxBlock],
    stories: &DocxStories,
    budget: &mut Budget<'_>,
) -> Result<String> {
    let mut output = TextOutput::new(budget);
    for header in &stories.headers {
        text_blocks(&mut output, &header.blocks)?;
    }
    output.section_break()?;
    text_blocks(&mut output, blocks)?;
    output.append_notes(stories, false)?;
    for footer in &stories.footers {
        output.section_break()?;
        text_blocks(&mut output, &footer.blocks)?;
    }
    Ok(output.finish())
}

fn text_blocks(output: &mut TextOutput<'_, '_>, blocks: &[DocxBlock]) -> Result<()> {
    for block in blocks {
        output.checkpoint()?;
        match block {
            DocxBlock::Paragraph(paragraph) => {
                if paragraph.runs.is_empty() {
                    continue;
                }
                output.start_line()?;
                output.append_plain_runs(&paragraph.runs)?;
            }
            DocxBlock::Table(table) => output.plain_table(table)?,
        }
    }
    Ok(())
}

/// Markdown in the same section order as text. Note references are
/// `[^label]` and definitions `[^label]: text`, the footnote syntax of GFM.
pub(in crate::nodes::extract) fn blocks_to_markdown(
    blocks: &[DocxBlock],
    stories: &DocxStories,
    budget: &mut Budget<'_>,
) -> Result<String> {
    let mut output = TextOutput::new(budget);
    for header in &stories.headers {
        markdown_blocks(&mut output, &header.blocks)?;
    }
    output.section_break()?;
    markdown_blocks(&mut output, blocks)?;
    output.append_notes(stories, true)?;
    for footer in &stories.footers {
        output.section_break()?;
        markdown_blocks(&mut output, &footer.blocks)?;
    }
    output.finish_trimmed()
}

fn markdown_blocks(output: &mut TextOutput<'_, '_>, blocks: &[DocxBlock]) -> Result<()> {
    let mut numbered_counters = HashMap::<u32, u32>::new();

    for block in blocks {
        output.checkpoint()?;
        match block {
            DocxBlock::Paragraph(paragraph) => {
                if paragraph.runs.is_empty() {
                    continue;
                }
                if let Some(level) = heading_level(paragraph) {
                    output.start_line()?;
                    output.start_line()?;
                    output.append_repeat('#', level, "DOCX markdown heading")?;
                    output.append(" ", "DOCX markdown heading")?;
                    let inherited = paragraph.style_emphasis;
                    output.append_markdown_runs(&paragraph.runs, false, inherited)?;
                    output.start_line()?;
                    continue;
                }
                if paragraph.is_list_item {
                    output.start_line()?;
                    output.append_indent(paragraph.list_level)?;
                    if paragraph.is_numbered {
                        let counter = numbered_counters.entry(paragraph.list_level).or_insert(0);
                        *counter = counter.checked_add(1).ok_or_else(|| {
                            anyhow::anyhow!("extract_word: numbered list counter overflow")
                        })?;
                        output.append_u32(*counter, "DOCX markdown list marker")?;
                        output.append(". ", "DOCX markdown list marker")?;
                    } else {
                        output.append("- ", "DOCX markdown list marker")?;
                    }
                    output.append_markdown_runs(&paragraph.runs, false, Flags::default())?;
                    continue;
                }

                numbered_counters.clear();
                output.start_line()?;
                output.append_markdown_runs(&paragraph.runs, false, Flags::default())?;
            }
            DocxBlock::Table(table) => {
                numbered_counters.clear();
                output.markdown_table(table)?;
            }
        }
    }
    Ok(())
}

fn heading_level(paragraph: &DocxParagraph) -> Option<usize> {
    match paragraph.style.as_deref()? {
        "Heading1" | "heading1" | "Title" => Some(1),
        "Heading2" | "heading2" | "Subtitle" => Some(2),
        "Heading3" | "heading3" => Some(3),
        "Heading4" | "heading4" => Some(4),
        "Heading5" | "heading5" => Some(5),
        "Heading6" | "heading6" => Some(6),
        _ => None,
    }
}
