mod output;

use std::collections::HashMap;

use anyhow::Result;

use self::output::TextOutput;
use super::super::docx_parser::{DocxBlock, DocxParagraph};
use super::super::resource::Budget;

pub(in crate::nodes::extract) fn blocks_to_text(
    blocks: &[DocxBlock],
    budget: &mut Budget<'_>,
) -> Result<String> {
    let mut output = TextOutput::new(budget);
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
    Ok(output.finish())
}

pub(in crate::nodes::extract) fn blocks_to_markdown(
    blocks: &[DocxBlock],
    budget: &mut Budget<'_>,
) -> Result<String> {
    let mut output = TextOutput::new(budget);
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
                    output.append_markdown_runs(&paragraph.runs, false)?;
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
                    output.append_markdown_runs(&paragraph.runs, false)?;
                    continue;
                }

                numbered_counters.clear();
                output.start_line()?;
                output.append_markdown_runs(&paragraph.runs, false)?;
            }
            DocxBlock::Table(table) => {
                numbered_counters.clear();
                output.markdown_table(table)?;
            }
        }
    }

    output.finish_trimmed()
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
