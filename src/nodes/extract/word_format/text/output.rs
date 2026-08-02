use std::fmt::Write as _;

use anyhow::{Context, Result};

use super::super::super::docx_parser::{DocxCell, DocxRun, DocxTable};
use super::super::super::resource::Budget;

pub(super) struct TextOutput<'a, 'b> {
    value: String,
    has_line: bool,
    budget: &'a mut Budget<'b>,
}

impl<'a, 'b> TextOutput<'a, 'b> {
    pub(super) fn new(budget: &'a mut Budget<'b>) -> Self {
        Self {
            value: String::new(),
            has_line: false,
            budget,
        }
    }

    pub(super) fn checkpoint(&self) -> Result<()> {
        self.budget.checkpoint()
    }

    pub(super) fn append(&mut self, value: &str, what: &str) -> Result<()> {
        self.budget.charge_output(value.len() as u64, what)?;
        self.value
            .try_reserve(value.len())
            .with_context(|| format!("extract_word: cannot reserve memory for {what}"))?;
        self.value.push_str(value);
        Ok(())
    }

    pub(super) fn append_repeat(&mut self, value: char, count: usize, what: &str) -> Result<()> {
        self.budget.charge_output(count as u64, what)?;
        self.value
            .try_reserve(count)
            .with_context(|| format!("extract_word: cannot reserve memory for {what}"))?;
        self.value.extend(std::iter::repeat_n(value, count));
        Ok(())
    }

    pub(super) fn append_u32(&mut self, value: u32, what: &str) -> Result<()> {
        let digits = if value == 0 {
            1
        } else {
            value.ilog10() as usize + 1
        };
        self.budget.charge_output(digits as u64, what)?;
        self.value
            .try_reserve(digits)
            .with_context(|| format!("extract_word: cannot reserve memory for {what}"))?;
        write!(&mut self.value, "{value}")
            .map_err(|_| anyhow::anyhow!("extract_word: failed to format list marker"))
    }

    pub(super) fn start_line(&mut self) -> Result<()> {
        if self.has_line {
            self.append("\n", "DOCX formatted line separators")?;
        } else {
            self.has_line = true;
        }
        Ok(())
    }

    pub(super) fn append_plain_runs(&mut self, runs: &[DocxRun]) -> Result<()> {
        for run in runs {
            self.checkpoint()?;
            self.append(&run.text, "DOCX formatted text copies")?;
        }
        Ok(())
    }

    pub(super) fn plain_table(&mut self, table: &DocxTable) -> Result<()> {
        for row in &table.rows {
            self.checkpoint()?;
            self.start_line()?;
            for (cell_index, cell) in row.cells.iter().enumerate() {
                self.checkpoint()?;
                if cell_index > 0 {
                    self.append(" | ", "DOCX plain-text table separators")?;
                }
                let mut has_paragraph = false;
                for paragraph in &cell.paragraphs {
                    self.checkpoint()?;
                    if !self.paragraph_has_text(&paragraph.runs)? {
                        continue;
                    }
                    if has_paragraph {
                        self.append(" ", "DOCX plain-text table separators")?;
                    }
                    self.append_plain_runs(&paragraph.runs)?;
                    has_paragraph = true;
                }
            }
        }
        Ok(())
    }

    pub(super) fn append_indent(&mut self, level: u32) -> Result<()> {
        let spaces = u64::from(level)
            .checked_mul(2)
            .ok_or_else(|| anyhow::anyhow!("extract_word: markdown indentation size overflow"))?;
        let spaces = usize::try_from(spaces)
            .map_err(|_| anyhow::anyhow!("extract_word: markdown indentation is too large"))?;
        self.append_repeat(' ', spaces, "DOCX markdown indentation")
    }

    pub(super) fn append_markdown_runs(
        &mut self,
        runs: &[DocxRun],
        escape_pipe: bool,
    ) -> Result<()> {
        for run in runs {
            self.checkpoint()?;
            if run.text.is_empty() {
                continue;
            }
            let emphasis = match (run.bold, run.italic) {
                (true, true) => "***",
                (true, false) => "**",
                (false, true) => "*",
                (false, false) => "",
            };
            self.append(emphasis, "DOCX markdown run wrappers")?;
            if run.strikethrough {
                self.append("~~", "DOCX markdown run wrappers")?;
            }
            if escape_pipe {
                self.append_escaped_cell_text(&run.text)?;
            } else {
                self.append(&run.text, "DOCX markdown text copies")?;
            }
            if run.strikethrough {
                self.append("~~", "DOCX markdown run wrappers")?;
            }
            self.append(emphasis, "DOCX markdown run wrappers")?;
        }
        Ok(())
    }

    fn append_escaped_cell_text(&mut self, value: &str) -> Result<()> {
        let mut segments = value.split('|').peekable();
        while let Some(segment) = segments.next() {
            self.checkpoint()?;
            self.append(segment, "DOCX markdown table text copies")?;
            if segments.peek().is_some() {
                self.append("\\|", "DOCX markdown table escaping")?;
            }
        }
        Ok(())
    }

    pub(super) fn markdown_table(&mut self, table: &DocxTable) -> Result<()> {
        let Some(header) = table.rows.first() else {
            return Ok(());
        };
        let mut column_count = 0;
        for row in &table.rows {
            self.checkpoint()?;
            column_count = column_count.max(row.cells.len());
        }
        if column_count == 0 {
            return Ok(());
        }

        self.start_line()?;
        self.start_line()?;
        self.markdown_row(&header.cells, column_count)?;
        self.start_line()?;
        self.append("|", "DOCX markdown table separators")?;
        for _ in 0..column_count {
            self.checkpoint()?;
            self.append(" --- |", "DOCX markdown table separators")?;
        }
        for row in &table.rows[1..] {
            self.checkpoint()?;
            self.start_line()?;
            self.markdown_row(&row.cells, column_count)?;
        }
        self.start_line()
    }

    fn markdown_row(&mut self, cells: &[DocxCell], column_count: usize) -> Result<()> {
        self.append("| ", "DOCX markdown table separators")?;
        for index in 0..column_count {
            self.checkpoint()?;
            if index > 0 {
                self.append(" | ", "DOCX markdown table separators")?;
            }
            if let Some(cell) = cells.get(index) {
                self.markdown_cell(cell)?;
            }
        }
        self.append(" |", "DOCX markdown table separators")
    }

    fn markdown_cell(&mut self, cell: &DocxCell) -> Result<()> {
        let mut has_paragraph = false;
        for paragraph in &cell.paragraphs {
            self.checkpoint()?;
            if !self.paragraph_has_text(&paragraph.runs)? {
                continue;
            }
            if has_paragraph {
                self.append("<br>", "DOCX markdown table separators")?;
            }
            self.append_markdown_runs(&paragraph.runs, true)?;
            has_paragraph = true;
        }
        Ok(())
    }

    fn paragraph_has_text(&self, runs: &[DocxRun]) -> Result<bool> {
        for run in runs {
            self.checkpoint()?;
            if !run.text.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn finish(self) -> String {
        self.value
    }

    pub(super) fn finish_trimmed(mut self) -> Result<String> {
        self.checkpoint()?;
        let (start, length) = {
            let trimmed = self.value.trim();
            let start = trimmed.as_ptr() as usize - self.value.as_ptr() as usize;
            (start, trimmed.len())
        };
        if start > 0 {
            self.value.drain(..start);
        }
        self.value.truncate(length);
        Ok(self.value)
    }
}
