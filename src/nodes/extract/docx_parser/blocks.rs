//! Streaming assembly of DOCX paragraphs, runs and tables from the
//! `word/document.xml` event sequence.

use std::collections::HashMap;

use anyhow::Result;
use quick_xml::events::BytesStart;

use super::super::resource::Budget;
use super::properties::{apply_paragraph_property, apply_run_property};
use super::{DocxBlock, DocxCell, DocxParagraph, DocxRow, DocxRun, DocxTable};

/// Elements whose content is never live. An `mc:Fallback` repeats its
/// `mc:Choice` in an older vocabulary, and a tracked formatting change holds
/// the previous `w:rPr` or `w:pPr`, not the current one.
const SKIPPED: [&str; 3] = ["mc:Fallback", "w:rPrChange", "w:pPrChange"];

/// Document-wide lookups resolved before the walk starts.
pub(super) struct Lookups<'a> {
    pub(super) numbering_defs: &'a HashMap<String, bool>,
    pub(super) theme_colors: &'a HashMap<String, String>,
}

/// One open `w:p`. A text box nests whole paragraphs inside a run of its
/// host paragraph, so paragraph and run state is kept per open paragraph.
#[derive(Default)]
struct Frame {
    paragraph: DocxParagraph,
    run: Option<DocxRun>,
    in_text: bool,
    in_run_props: bool,
    in_para_props: bool,
    /// Text-box content anchored in this paragraph, emitted after it.
    boxed: Vec<DocxBlock>,
    /// Open cells and tables when the paragraph started. Anything deeper was
    /// opened by a text box inside this paragraph.
    cell_depth: usize,
    table_depth: usize,
}

/// Parser state for one document walk. The stacks hold the open paragraphs
/// and the partially built tables, rows and cells.
#[derive(Default)]
pub(super) struct BlockBuilder {
    blocks: Vec<DocxBlock>,
    frames: Vec<Frame>,
    /// Open `SKIPPED` elements; everything inside them is ignored.
    skip_depth: u64,
    table_stack: Vec<DocxTable>,
    row_stack: Vec<DocxRow>,
    cell_stack: Vec<DocxCell>,
}

impl BlockBuilder {
    /// Handle a start or empty element.
    pub(super) fn open(
        &mut self,
        event: &BytesStart<'_>,
        is_empty: bool,
        version: quick_xml::XmlVersion,
        lookups: &Lookups<'_>,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        let name = event.name();
        let name = name.as_ref();
        if SKIPPED.contains(&name) {
            self.skip_depth += u64::from(!is_empty);
            return Ok(());
        }
        if self.skip_depth > 0 {
            return Ok(());
        }
        match name {
            // An empty table, row or cell has no end tag to close it.
            "w:tbl" | "w:tr" | "w:tc" => {
                self.open_table_part(name);
                if is_empty {
                    self.close(name);
                }
                Ok(())
            }
            "w:p" | "w:r" | "w:t" | "w:tab" | "w:br" => self.open_text(name, is_empty, budget),
            "w:pPr" | "w:pStyle" | "w:numPr" | "w:ilvl" | "w:numId" | "w:rPr" | "w:b" | "w:i"
            | "w:u" | "w:strike" | "w:color" | "w:highlight" => {
                self.open_property(name, event, is_empty, version, lookups, budget)
            }
            _ => Ok(()),
        }
    }

    fn open_table_part(&mut self, name: &str) {
        match name {
            "w:tbl" => self.table_stack.push(DocxTable::default()),
            "w:tr" if !self.table_stack.is_empty() => self.row_stack.push(DocxRow::default()),
            "w:tc" if !self.row_stack.is_empty() => self.cell_stack.push(DocxCell::default()),
            _ => {}
        }
    }

    /// Paragraphs, runs and run content. An empty `w:p` or `w:r` has no end
    /// tag and no content, so it opens nothing.
    fn open_text(&mut self, name: &str, is_empty: bool, budget: &mut Budget<'_>) -> Result<()> {
        if name == "w:p" {
            if !is_empty {
                self.frames.push(Frame {
                    cell_depth: self.cell_stack.len(),
                    table_depth: self.table_stack.len(),
                    ..Frame::default()
                });
            }
            return Ok(());
        }
        let Some(frame) = self.frames.last_mut() else {
            return Ok(());
        };
        if name == "w:r" {
            if !is_empty {
                frame.run = Some(DocxRun::default());
                frame.in_run_props = false;
            }
            return Ok(());
        }
        let Some(run) = frame.run.as_mut() else {
            return Ok(());
        };
        match name {
            "w:t" => frame.in_text = !is_empty,
            "w:tab" => {
                budget.charge_output(1, "DOCX extracted text")?;
                run.text.push('\t');
            }
            "w:br" => {
                budget.charge_output(1, "DOCX extracted text")?;
                run.text.push('\n');
            }
            _ => {}
        }
        Ok(())
    }

    /// `w:pPr`, `w:rPr` and their children. Run properties inside `w:pPr`
    /// describe the paragraph mark, not a run, and are ignored.
    fn open_property(
        &mut self,
        name: &str,
        event: &BytesStart<'_>,
        is_empty: bool,
        version: quick_xml::XmlVersion,
        lookups: &Lookups<'_>,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        let Some(frame) = self.frames.last_mut() else {
            return Ok(());
        };
        match (name, frame.run.as_mut()) {
            ("w:pPr", _) => frame.in_para_props = !is_empty,
            ("w:rPr", Some(_)) => frame.in_run_props = !is_empty,
            (_, Some(run)) if frame.in_run_props => {
                apply_run_property(name, event, run, version, lookups.theme_colors, budget)?;
            }
            _ if frame.in_para_props => {
                let paragraph = &mut frame.paragraph;
                apply_paragraph_property(name, event, paragraph, version, lookups, budget)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Append text that belongs to an open `w:t` inside a run.
    pub(super) fn text(&mut self, text: &str) {
        if self.skip_depth == 0
            && let Some(frame) = self.frames.last_mut()
            && frame.in_text
            && let Some(run) = frame.run.as_mut()
        {
            run.text.push_str(text);
        }
    }

    /// Handle an end element.
    pub(super) fn close(&mut self, name: &str) {
        if SKIPPED.contains(&name) {
            self.skip_depth = self.skip_depth.saturating_sub(1);
            return;
        }
        if self.skip_depth > 0 {
            return;
        }
        match name {
            "w:p" => {
                if let Some(frame) = self.frames.pop() {
                    self.finish_paragraph(frame);
                }
            }
            "w:t" | "w:r" | "w:rPr" | "w:pPr" => self.close_in_paragraph(name),
            "w:tc" => {
                if let Some(cell) = self.cell_stack.pop()
                    && let Some(row) = self.row_stack.last_mut()
                {
                    row.cells.push(cell);
                }
            }
            "w:tr" => {
                if let Some(row) = self.row_stack.pop()
                    && let Some(table) = self.table_stack.last_mut()
                {
                    table.rows.push(row);
                }
            }
            "w:tbl" => self.finish_table(),
            _ => {}
        }
    }

    fn close_in_paragraph(&mut self, name: &str) {
        let Some(frame) = self.frames.last_mut() else {
            return;
        };
        match name {
            "w:t" => frame.in_text = false,
            "w:r" => {
                frame.in_text = false;
                frame.in_run_props = false;
                if let Some(run) = frame.run.take()
                    && !run.text.is_empty()
                {
                    frame.paragraph.runs.push(run);
                }
            }
            "w:rPr" => frame.in_run_props = false,
            "w:pPr" => frame.in_para_props = false,
            _ => {}
        }
    }

    /// A paragraph directly inside a text box follows the paragraph that
    /// anchors the box; any other paragraph goes to its cell or the document.
    fn finish_paragraph(&mut self, frame: Frame) {
        let Frame {
            paragraph, boxed, ..
        } = frame;
        if let Some(host) = self.frames.last_mut()
            && host.cell_depth == self.cell_stack.len()
        {
            if !paragraph.runs.is_empty() {
                host.boxed.push(DocxBlock::Paragraph(paragraph));
            }
            host.boxed.extend(boxed);
        } else if let Some(cell) = self.cell_stack.last_mut() {
            cell.paragraphs.push(paragraph);
            for block in boxed {
                match block {
                    DocxBlock::Paragraph(paragraph) => cell.paragraphs.push(paragraph),
                    DocxBlock::Table(table) => cell.paragraphs.extend(table_paragraphs(table)),
                }
            }
        } else {
            self.blocks.push(DocxBlock::Paragraph(paragraph));
            self.blocks.extend(boxed);
        }
    }

    /// A table inside a text box follows the paragraph that anchors the box.
    /// Otherwise a top-level table becomes a block and a nested table's
    /// paragraphs flatten into the enclosing cell.
    fn finish_table(&mut self) {
        let Some(table) = self.table_stack.pop() else {
            return;
        };
        if let Some(host) = self.frames.last_mut()
            && host.table_depth == self.table_stack.len()
        {
            host.boxed.push(DocxBlock::Table(table));
        } else if self.table_stack.is_empty() {
            self.blocks.push(DocxBlock::Table(table));
        } else if let Some(parent_cell) = self.cell_stack.last_mut() {
            parent_cell.paragraphs.extend(table_paragraphs(table));
        }
    }

    pub(super) fn finish(self) -> Vec<DocxBlock> {
        self.blocks
    }
}

fn table_paragraphs(table: DocxTable) -> impl Iterator<Item = DocxParagraph> {
    table
        .rows
        .into_iter()
        .flat_map(|row| row.cells)
        .flat_map(|cell| cell.paragraphs)
}
