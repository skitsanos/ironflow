//! Streaming assembly of DOCX paragraphs, runs and tables from the
//! `word/document.xml` event sequence.

use std::collections::HashMap;

use anyhow::Result;
use quick_xml::events::BytesStart;

use super::super::resource::Budget;
use super::properties::apply_run_color;
use super::xml::visit_attributes;
use super::{DocxBlock, DocxCell, DocxParagraph, DocxRow, DocxRun, DocxTable};

const PART: &str = "word/document.xml";

/// Document-wide lookups resolved before the walk starts.
pub(super) struct Lookups<'a> {
    pub(super) numbering_defs: &'a HashMap<String, bool>,
    pub(super) theme_colors: &'a HashMap<String, String>,
}

/// Parser state for one document walk. The flags track which element kinds
/// are open; the stacks hold partially built tables, rows and cells.
#[derive(Default)]
pub(super) struct BlockBuilder {
    blocks: Vec<DocxBlock>,
    in_paragraph: bool,
    in_run: bool,
    in_text: bool,
    in_run_props: bool,
    in_para_props: bool,
    table_stack: Vec<DocxTable>,
    row_stack: Vec<DocxRow>,
    cell_stack: Vec<DocxCell>,
    current_para: DocxParagraph,
    current_run: DocxRun,
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
        match name {
            "w:tbl" | "w:tr" | "w:tc" | "w:p" | "w:r" | "w:t" | "w:tab" | "w:br" => {
                self.open_structure(name, is_empty, budget)
            }
            "w:pPr" | "w:pStyle" | "w:numPr" | "w:ilvl" | "w:numId" => {
                self.open_paragraph_property(name, event, version, lookups, budget)
            }
            "w:rPr" | "w:b" | "w:i" | "w:u" | "w:strike" | "w:color" | "w:highlight" => {
                self.open_run_property(name, event, version, lookups, budget)
            }
            _ => Ok(()),
        }
    }

    fn open_structure(
        &mut self,
        name: &str,
        is_empty: bool,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        match name {
            "w:tbl" => self.table_stack.push(DocxTable::default()),
            "w:tr" if !self.table_stack.is_empty() => self.row_stack.push(DocxRow::default()),
            "w:tc" if !self.row_stack.is_empty() => self.cell_stack.push(DocxCell::default()),
            "w:p" => {
                self.in_paragraph = true;
                self.current_para = DocxParagraph::default();
            }
            "w:r" if self.in_paragraph => {
                self.in_run = true;
                self.current_run = DocxRun::default();
            }
            "w:t" if self.in_run => self.in_text = !is_empty,
            "w:tab" if self.in_run => {
                budget.charge_output(1, "DOCX extracted text")?;
                self.current_run.text.push('\t');
            }
            "w:br" if self.in_run => {
                budget.charge_output(1, "DOCX extracted text")?;
                self.current_run.text.push('\n');
            }
            _ => {}
        }
        Ok(())
    }

    fn open_paragraph_property(
        &mut self,
        name: &str,
        event: &BytesStart<'_>,
        version: quick_xml::XmlVersion,
        lookups: &Lookups<'_>,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        match name {
            "w:pPr" if self.in_paragraph => self.in_para_props = true,
            "w:pStyle" if self.in_para_props => {
                let style = &mut self.current_para.style;
                visit_attributes(event, PART, version, budget, |key, value, _| {
                    if key == b"w:val" {
                        *style = Some(String::from_utf8_lossy(value).to_string());
                    }
                    Ok(())
                })?;
            }
            "w:numPr" if self.in_para_props => self.current_para.is_list_item = true,
            "w:ilvl" if self.in_para_props => {
                let list_level = &mut self.current_para.list_level;
                visit_attributes(event, PART, version, budget, |key, value, budget| {
                    if key == b"w:val"
                        && let Ok(level) = String::from_utf8_lossy(value).parse::<u32>()
                    {
                        budget.charge_items(u64::from(level), "DOCX list indentation work")?;
                        *list_level = level;
                    }
                    Ok(())
                })?;
            }
            "w:numId" if self.in_para_props => {
                let is_numbered = &mut self.current_para.is_numbered;
                let numbering_defs = lookups.numbering_defs;
                visit_attributes(event, PART, version, budget, |key, value, _| {
                    if key == b"w:val" {
                        *is_numbered = numbering_defs
                            .get(String::from_utf8_lossy(value).as_ref())
                            .copied()
                            .unwrap_or(false);
                    }
                    Ok(())
                })?;
            }
            _ => {}
        }
        Ok(())
    }

    fn open_run_property(
        &mut self,
        name: &str,
        event: &BytesStart<'_>,
        version: quick_xml::XmlVersion,
        lookups: &Lookups<'_>,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        match name {
            "w:rPr" if self.in_run => self.in_run_props = true,
            "w:b" if self.in_run_props => self.current_run.bold = true,
            "w:i" if self.in_run_props => self.current_run.italic = true,
            "w:u" if self.in_run_props => self.current_run.underline = true,
            "w:strike" if self.in_run_props => self.current_run.strikethrough = true,
            "w:color" if self.in_run_props => {
                apply_run_color(
                    event,
                    &mut self.current_run,
                    lookups.theme_colors,
                    version,
                    budget,
                )?;
            }
            "w:highlight" if self.in_run_props => {
                let highlight = &mut self.current_run.highlight;
                visit_attributes(event, PART, version, budget, |key, value, _| {
                    if key == b"w:val" {
                        let value = String::from_utf8_lossy(value).to_string();
                        if value != "none" && !value.is_empty() {
                            *highlight = Some(value);
                        }
                    }
                    Ok(())
                })?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Append text that belongs to an open `w:t` inside a run.
    pub(super) fn text(&mut self, text: &str) {
        if self.in_run && self.in_text {
            self.current_run.text.push_str(text);
        }
    }

    /// Handle an end element.
    pub(super) fn close(&mut self, name: &str) {
        match name {
            "w:t" => self.in_text = false,
            "w:p" => {
                self.in_paragraph = false;
                let finished = std::mem::take(&mut self.current_para);
                if let Some(cell) = self.cell_stack.last_mut() {
                    cell.paragraphs.push(finished);
                } else {
                    self.blocks.push(DocxBlock::Paragraph(finished));
                }
            }
            "w:r" => {
                self.in_run = false;
                if self.current_run.text.is_empty() {
                    self.current_run = DocxRun::default();
                } else {
                    self.current_para
                        .runs
                        .push(std::mem::take(&mut self.current_run));
                }
            }
            "w:rPr" => self.in_run_props = false,
            "w:pPr" => self.in_para_props = false,
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

    /// A top-level table becomes a block; a nested table's paragraphs flatten
    /// into the enclosing cell.
    fn finish_table(&mut self) {
        let Some(table) = self.table_stack.pop() else {
            return;
        };
        if self.table_stack.is_empty() {
            self.blocks.push(DocxBlock::Table(table));
        } else if let Some(parent_cell) = self.cell_stack.last_mut() {
            for row in table.rows {
                for mut cell in row.cells {
                    parent_cell.paragraphs.append(&mut cell.paragraphs);
                }
            }
        }
    }

    pub(super) fn finish(self) -> Vec<DocxBlock> {
        self.blocks
    }
}
