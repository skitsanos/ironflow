//! Section breaks and note definitions for the text and Markdown formats.

use anyhow::Result;

use super::super::super::docx_parser::{DocxBlock, DocxNote, DocxParagraph, DocxStories, Flags};
use super::output::TextOutput;

impl TextOutput<'_, '_> {
    /// Leave a blank line before the next section, if anything precedes it.
    pub(super) fn section_break(&mut self) -> Result<()> {
        if self.has_line() {
            self.start_line()?;
        }
        Ok(())
    }

    /// One `[^label]: text` line per note, footnotes before endnotes. A
    /// note's paragraphs, table cells included, are joined by a space.
    pub(super) fn append_notes(&mut self, stories: &DocxStories, markdown: bool) -> Result<()> {
        if stories.footnotes.is_empty() && stories.endnotes.is_empty() {
            return Ok(());
        }
        self.section_break()?;
        for note in stories.footnotes.iter().chain(&stories.endnotes) {
            self.checkpoint()?;
            self.append_note(note, markdown)?;
        }
        Ok(())
    }

    fn append_note(&mut self, note: &DocxNote, markdown: bool) -> Result<()> {
        self.start_line()?;
        self.append("[^", "DOCX note definitions")?;
        self.append(&note.label, "DOCX note definitions")?;
        self.append("]:", "DOCX note definitions")?;
        let mut paragraphs = Vec::new();
        collect_paragraphs(&note.blocks, &mut paragraphs);
        for paragraph in paragraphs {
            self.checkpoint()?;
            if paragraph.runs.is_empty() {
                continue;
            }
            self.append(" ", "DOCX note definitions")?;
            if markdown {
                self.append_markdown_runs(&paragraph.runs, false, Flags::default())?;
            } else {
                self.append_plain_runs(&paragraph.runs)?;
            }
        }
        Ok(())
    }
}

fn collect_paragraphs<'b>(blocks: &'b [DocxBlock], paragraphs: &mut Vec<&'b DocxParagraph>) {
    for block in blocks {
        match block {
            DocxBlock::Paragraph(paragraph) => paragraphs.push(paragraph),
            DocxBlock::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        paragraphs.extend(&cell.paragraphs);
                    }
                }
            }
        }
    }
}
