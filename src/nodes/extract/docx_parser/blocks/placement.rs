//! Where a finished paragraph or table goes: the document, the enclosing
//! table cell, or after the paragraph that anchors its text box.

use super::super::{DocxBlock, DocxParagraph, DocxTable};
use super::{BlockBuilder, Frame};

impl BlockBuilder {
    /// A paragraph directly inside a text box follows the paragraph that
    /// anchors the box; any other paragraph goes to its cell or the document.
    pub(super) fn finish_paragraph(&mut self, frame: Frame) {
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
    pub(super) fn finish_table(&mut self) {
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
}

fn table_paragraphs(table: DocxTable) -> impl Iterator<Item = DocxParagraph> {
    table
        .rows
        .into_iter()
        .flat_map(|row| row.cells)
        .flat_map(|cell| cell.paragraphs)
}
