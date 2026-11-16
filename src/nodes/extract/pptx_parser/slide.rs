use std::io::BufRead;

use anyhow::Result;
use quick_xml::events::{BytesStart, Event};

use self::attributes::{
    collect_embed_id, collect_list_level, collect_string_attribute, local_name,
};
use super::{PptxElement, PptxTextPara};
use crate::nodes::extract::resource::Budget;
use crate::util::xml::Decoder;

mod attributes;
mod bullets;

#[derive(Default)]
struct State {
    title: Option<String>,
    elements: Vec<PptxElement>,
    placeholder: Option<String>,
    in_text_body: bool,
    in_paragraph: bool,
    current_text: String,
    bullets: bullets::Bullets,
    /// `a:br` elements seen since the last text of the open paragraph. They
    /// become line breaks only when more text follows in the same paragraph.
    pending_breaks: usize,
    /// The open table cell has a line break or a paragraph end to render as
    /// a space before its next text.
    cell_break: bool,
    current_paragraphs: Vec<PptxTextPara>,
    in_run: bool,
    in_text: bool,
    in_table: bool,
    table_rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell_text: String,
    in_cell: bool,
    in_picture: bool,
    picture_alt: Option<String>,
    picture_embed_id: Option<String>,
}

pub(super) fn parse_pptx_slide<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<(Option<String>, Vec<PptxElement>)> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut decoder = Decoder::default();
    let mut buffer = Vec::new();
    let mut state = State::default();
    let mut depth = 0_u64;
    let mut saw_element = false;

    loop {
        budget.checkpoint()?;
        match reader
            .read_event_into(&mut buffer)
            .map_err(anyhow::Error::from)
            .and_then(|event| {
                decoder.decode(event, |bytes| {
                    budget.charge_output(bytes, "PPTX decoded slide text")
                })
            }) {
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                budget.charge_item("PPTX slide XML events")?;
                start_element(&event, &mut state, decoder.version, budget)?;
            }
            Ok(Event::Empty(event)) => {
                saw_element = true;
                budget.charge_item("PPTX slide XML events")?;
                start_element(&event, &mut state, decoder.version, budget)?;
                end_element(event.name().as_ref().as_bytes(), &mut state, budget)?;
            }
            Ok(Event::Text(event)) if state.in_text => {
                budget.charge_item("PPTX slide XML events")?;
                push_text(event.as_ref(), &mut state, budget)?;
            }
            Ok(Event::End(event)) => {
                budget.charge_item("PPTX slide XML events")?;
                depth = depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_pptx: invalid unmatched closing element in slide")
                })?;
                end_element(event.name().as_ref().as_bytes(), &mut state, budget)?;
            }
            Ok(Event::Eof) => break,
            Ok(_) => budget.charge_item("PPTX slide XML events")?,
            Err(error) => anyhow::bail!("extract_pptx: invalid XML in slide: {error}"),
        }
        buffer.clear();
    }
    if !saw_element || depth != 0 {
        anyhow::bail!("extract_pptx: incomplete XML in slide part");
    }
    Ok((state.title, state.elements))
}

fn start_element(
    event: &BytesStart<'_>,
    state: &mut State,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<()> {
    match local_name(event.name().as_ref().as_bytes()) {
        b"sp" => {
            state.placeholder = None;
            state.current_paragraphs.clear();
            state.bullets.start_shape();
        }
        b"graphicFrame" => state.bullets.start_shape(),
        b"ph" => {
            state.bullets.placeholder();
            collect_string_attribute(event, b"type", &mut state.placeholder, version, budget)?;
        }
        b"txBody" => {
            state.in_text_body = true;
            state.bullets.start_text_body();
        }
        b"p" if state.in_text_body => {
            state.in_paragraph = true;
            state.current_text.clear();
            state.bullets.start_paragraph();
        }
        b"r" if state.in_paragraph => state.in_run = true,
        b"br" if state.in_paragraph => {
            state.pending_breaks = state.pending_breaks.saturating_add(1);
            state.cell_break = state.in_cell;
        }
        b"t" if state.in_run => state.in_text = true,
        b"pPr" if state.in_paragraph => collect_list_level(event, state, version, budget)?,
        b"tbl" => {
            state.in_table = true;
            state.table_rows.clear();
        }
        b"tr" if state.in_table => state.current_row.clear(),
        b"tc" if state.in_table => {
            state.in_cell = true;
            state.cell_break = false;
            state.current_cell_text.clear();
        }
        b"pic" => {
            state.in_picture = true;
            state.picture_alt = None;
            state.picture_embed_id = None;
        }
        b"cNvPr" if state.in_picture => {
            collect_string_attribute(event, b"descr", &mut state.picture_alt, version, budget)?;
        }
        b"blip" if state.in_picture => collect_embed_id(event, state, version, budget)?,
        name if state.in_text_body => state.bullets.open(name, state.in_paragraph),
        _ => {}
    }
    Ok(())
}

fn end_element(name: &[u8], state: &mut State, budget: &mut Budget<'_>) -> Result<()> {
    match local_name(name) {
        b"t" => state.in_text = false,
        b"r" => state.in_run = false,
        b"p" if state.in_text_body => finish_paragraph(state, budget)?,
        b"txBody" => state.in_text_body = false,
        b"sp" => finish_shape(state, budget)?,
        b"tc" if state.in_table => {
            state.in_cell = false;
            budget.charge_item("PPTX table cells")?;
            state
                .current_row
                .push(std::mem::take(&mut state.current_cell_text));
        }
        b"tr" if state.in_table => {
            if !state.current_row.is_empty() {
                budget.charge_item("PPTX table rows")?;
                state
                    .table_rows
                    .push(std::mem::take(&mut state.current_row));
            }
        }
        b"tbl" => {
            if !state.table_rows.is_empty() {
                budget.charge_item("PPTX elements")?;
                state.elements.push(PptxElement::Table {
                    rows: std::mem::take(&mut state.table_rows),
                });
            }
            state.in_table = false;
        }
        b"pic" => {
            budget.charge_item("PPTX elements")?;
            state.elements.push(PptxElement::Image {
                alt_text: state.picture_alt.take(),
                embed_id: state.picture_embed_id.take(),
                embedded_path: None,
                artifact: None,
            });
            state.in_picture = false;
        }
        name if state.in_text_body => state.bullets.close(name),
        _ => {}
    }
    Ok(())
}

/// Append run text. A pending `a:br` is a line break inside a paragraph and
/// a space inside a table cell, which is one line in every text format.
fn push_text(text: &str, state: &mut State, budget: &mut Budget<'_>) -> Result<()> {
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

fn finish_paragraph(state: &mut State, budget: &mut Budget<'_>) -> Result<()> {
    state.pending_breaks = 0;
    // The next paragraph of a table cell continues the cell's single line.
    state.cell_break = state.in_cell;
    if !state.current_text.trim().is_empty() {
        budget.charge_item("PPTX paragraphs")?;
        state.current_paragraphs.push(PptxTextPara {
            text: std::mem::take(&mut state.current_text),
            list_level: state.bullets.list_level(),
        });
    } else {
        state.current_text.clear();
    }
    state.in_paragraph = false;
    Ok(())
}

fn finish_shape(state: &mut State, budget: &mut Budget<'_>) -> Result<()> {
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
