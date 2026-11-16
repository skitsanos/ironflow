//! Closing a run of text, a paragraph and a shape of a slide.

use anyhow::Result;

use super::super::inherited::Inherited;
use super::super::{PptxElement, PptxTextPara};
use super::State;
use crate::nodes::extract::resource::Budget;

/// Append run text. A pending `a:br` is a line break inside a paragraph and
/// a space inside a table cell, which is one line in every text format.
pub(super) fn push_text(text: &str, state: &mut State, budget: &mut Budget<'_>) -> Result<()> {
    let breaks = std::mem::take(&mut state.pending_breaks);
    if breaks > 0 && !state.current_text.is_empty() {
        budget.charge_output(breaks as u64, "PPTX line breaks")?;
        state.current_text.extend(std::iter::repeat_n('\n', breaks));
    }
    state.current_text.push_str(text);
    if state.in_cell {
        budget.charge_output(text.len() as u64 + 1, "PPTX duplicated cell text")?;
        if std::mem::take(&mut state.cell_break) && !state.current_cell_text.is_empty() {
            state.current_cell_text.push(' ');
        }
        state.current_cell_text.push_str(text);
    }
    Ok(())
}

pub(super) fn finish_paragraph(
    state: &mut State,
    inherited: &Inherited,
    budget: &mut Budget<'_>,
) -> Result<()> {
    state.pending_breaks = 0;
    // The next paragraph of a table cell continues the cell's single line.
    state.cell_break = state.in_cell;
    if !state.current_text.trim().is_empty() {
        budget.charge_item("PPTX paragraphs")?;
        state.current_paragraphs.push(PptxTextPara {
            text: std::mem::take(&mut state.current_text),
            list_level: state.bullets.list_level(inherited),
        });
    } else {
        state.current_text.clear();
    }
    state.in_paragraph = false;
    Ok(())
}

pub(super) fn finish_shape(state: &mut State, budget: &mut Budget<'_>) -> Result<()> {
    if state.current_paragraphs.is_empty() {
        state.placeholder = None;
        return Ok(());
    }
    if matches!(state.placeholder.as_deref(), Some("title" | "ctrTitle")) {
        if state.title.is_none() {
            let separators = state.current_paragraphs.len().saturating_sub(1) as u64;
            budget.charge_output(separators, "PPTX generated title separators")?;
            state.title = Some(
                state
                    .current_paragraphs
                    .iter()
                    .map(|paragraph| paragraph.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
                    // A heading is one line: a break in the title is a space.
                    .replace('\n', " "),
            );
        }
        state.current_paragraphs.clear();
    } else {
        budget.charge_item("PPTX elements")?;
        state.elements.push(PptxElement::TextBlock {
            placeholder: state.placeholder.take(),
            paragraphs: std::mem::take(&mut state.current_paragraphs),
        });
    }
    state.placeholder = None;
    Ok(())
}
