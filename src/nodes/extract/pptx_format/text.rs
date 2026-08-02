use anyhow::Result;

use super::super::pptx_parser::{PptxElement, PptxSlide};
use super::super::resource::Budget;

pub(in crate::nodes::extract) fn pptx_slides_to_text(
    slides: &[PptxSlide],
    budget: &mut Budget<'_>,
) -> Result<String> {
    let mut output = TextOutput::new(budget);
    for slide in slides {
        output.checkpoint()?;
        output.generated_line(&format!("===== SLIDE {} =====", slide.slide_index))?;
        if let Some(title) = &slide.title {
            output.retained_line(title)?;
        }
        for element in &slide.elements {
            output.checkpoint()?;
            match element {
                PptxElement::TextBlock { paragraphs, .. } => {
                    for paragraph in paragraphs {
                        output.retained_line(&paragraph.text)?;
                    }
                }
                PptxElement::Table { rows } => {
                    for row in rows {
                        output.row(row, " | ")?;
                    }
                }
                PptxElement::Image { .. } => {}
            }
        }
        if let Some(notes) = &slide.speaker_notes {
            output.generated_line("--- NOTES ---")?;
            output.retained_line(notes)?;
        }
        for comment in &slide.comments {
            output.generated("[COMMENT by ")?;
            output.retained(comment.author.as_deref().unwrap_or("?"));
            if let Some(date) = &comment.date {
                output.generated(" @ ")?;
                output.retained(date);
            }
            output.generated("]: ")?;
            output.retained_line(&comment.text)?;
        }
    }
    Ok(output.finish())
}

pub(in crate::nodes::extract) fn pptx_slides_to_markdown(
    slides: &[PptxSlide],
    budget: &mut Budget<'_>,
) -> Result<String> {
    let mut output = TextOutput::new(budget);
    for slide in slides {
        output.checkpoint()?;
        output.generated_line(&format!("## Slide {}", slide.slide_index))?;
        output.generated_line("")?;
        if let Some(title) = &slide.title {
            output.generated("### ")?;
            output.retained_line(title)?;
            output.generated_line("")?;
        }
        for element in &slide.elements {
            output.checkpoint()?;
            match element {
                PptxElement::TextBlock { paragraphs, .. } => {
                    for paragraph in paragraphs {
                        if let Some(level) = paragraph.list_level {
                            output.indent(level)?;
                            output.generated("- ")?;
                        }
                        output.retained_line(&paragraph.text)?;
                    }
                    output.generated_line("")?;
                }
                PptxElement::Table { rows } => output.markdown_table(rows)?,
                PptxElement::Image { .. } => output.generated_line("*(image)*")?,
            }
        }
        if let Some(notes) = &slide.speaker_notes {
            output.generated_line("**Speaker notes:**")?;
            output.retained_line(notes)?;
            output.generated_line("")?;
        }
        for comment in &slide.comments {
            output.generated("> 💬 **")?;
            output.retained(comment.author.as_deref().unwrap_or("?"));
            output.generated("**")?;
            if let Some(date) = &comment.date {
                output.generated(" (")?;
                output.retained(date);
                output.generated(")")?;
            }
            output.generated(": ")?;
            output.retained_line(&comment.text)?;
        }
    }
    Ok(output.finish_trimmed())
}

struct TextOutput<'a, 'b> {
    value: String,
    budget: &'a mut Budget<'b>,
}

impl<'a, 'b> TextOutput<'a, 'b> {
    fn new(budget: &'a mut Budget<'b>) -> Self {
        Self {
            value: String::new(),
            budget,
        }
    }

    fn checkpoint(&self) -> Result<()> {
        self.budget.checkpoint()
    }

    fn generated(&mut self, value: &str) -> Result<()> {
        self.budget
            .charge_output(value.len() as u64, "PPTX generated text")?;
        self.value.push_str(value);
        Ok(())
    }

    fn retained(&mut self, value: &str) {
        self.value.push_str(value);
    }

    fn generated_line(&mut self, value: &str) -> Result<()> {
        self.generated(value)?;
        self.generated("\n")
    }

    fn retained_line(&mut self, value: &str) -> Result<()> {
        self.retained(value);
        self.generated("\n")
    }

    fn row(&mut self, row: &[String], separator: &str) -> Result<()> {
        for (index, cell) in row.iter().enumerate() {
            self.checkpoint()?;
            if index > 0 {
                self.generated(separator)?;
            }
            self.retained(cell);
        }
        self.generated("\n")
    }

    fn indent(&mut self, level: u32) -> Result<()> {
        let bytes = u64::from(level)
            .checked_mul(2)
            .ok_or_else(|| anyhow::anyhow!("extract_pptx: markdown indentation size overflow"))?;
        self.budget
            .charge_output(bytes, "PPTX markdown indentation")?;
        let bytes = usize::try_from(bytes)
            .map_err(|_| anyhow::anyhow!("extract_pptx: markdown indentation is too large"))?;
        self.value.extend(std::iter::repeat_n(' ', bytes));
        Ok(())
    }

    fn markdown_table(&mut self, rows: &[Vec<String>]) -> Result<()> {
        let Some(header) = rows.first() else {
            return Ok(());
        };
        self.markdown_row(header)?;
        self.generated("|")?;
        for _ in header {
            self.generated(" --- |")?;
        }
        self.generated("\n")?;
        for row in &rows[1..] {
            self.markdown_row(row)?;
        }
        self.generated("\n")
    }

    fn markdown_row(&mut self, row: &[String]) -> Result<()> {
        self.generated("| ")?;
        for (index, cell) in row.iter().enumerate() {
            self.checkpoint()?;
            if index > 0 {
                self.generated(" | ")?;
            }
            self.retained(cell);
        }
        self.generated_line(" |")
    }

    fn finish(mut self) -> String {
        if self.value.ends_with('\n') {
            self.value.pop();
        }
        self.value
    }

    fn finish_trimmed(mut self) -> String {
        let length = self.value.trim_end().len();
        self.value.truncate(length);
        self.value
    }
}
