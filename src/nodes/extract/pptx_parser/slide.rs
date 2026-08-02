use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};

use super::{PptxElement, PptxTextPara};
use crate::nodes::extract::resource::Budget;

#[derive(Default)]
struct State {
    title: Option<String>,
    elements: Vec<PptxElement>,
    placeholder: Option<String>,
    in_text_body: bool,
    in_paragraph: bool,
    current_text: String,
    current_list_level: Option<u32>,
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
    let mut buffer = Vec::new();
    let mut state = State::default();
    let mut depth = 0_u64;
    let mut saw_element = false;

    loop {
        budget.checkpoint()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                budget.charge_item("PPTX slide XML events")?;
                start_element(&event, &mut state, budget)?;
            }
            Ok(Event::Empty(event)) => {
                saw_element = true;
                budget.charge_item("PPTX slide XML events")?;
                start_element(&event, &mut state, budget)?;
                end_element(event.name().as_ref(), &mut state, budget)?;
            }
            Ok(Event::Text(event)) if state.in_text => {
                budget.charge_item("PPTX slide XML events")?;
                budget.charge_output(event.len() as u64, "PPTX retained slide text")?;
                let text = String::from_utf8_lossy(event.as_ref());
                state.current_text.push_str(&text);
                if state.in_cell {
                    state.current_cell_text.push_str(&text);
                }
            }
            Ok(Event::End(event)) => {
                budget.charge_item("PPTX slide XML events")?;
                depth = depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_pptx: invalid unmatched closing element in slide")
                })?;
                end_element(event.name().as_ref(), &mut state, budget)?;
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

fn start_element(event: &BytesStart<'_>, state: &mut State, budget: &mut Budget<'_>) -> Result<()> {
    match local_name(event.name().as_ref()) {
        b"sp" => {
            state.placeholder = None;
            state.current_paragraphs.clear();
        }
        b"ph" => collect_string_attribute(event, b"type", &mut state.placeholder, budget)?,
        b"txBody" => state.in_text_body = true,
        b"p" if state.in_text_body => {
            state.in_paragraph = true;
            state.current_text.clear();
            state.current_list_level = None;
        }
        b"r" if state.in_paragraph => state.in_run = true,
        b"t" if state.in_run => state.in_text = true,
        b"pPr" if state.in_paragraph => collect_list_level(event, state)?,
        b"tbl" => {
            state.in_table = true;
            state.table_rows.clear();
        }
        b"tr" if state.in_table => state.current_row.clear(),
        b"tc" if state.in_table => {
            state.in_cell = true;
            state.current_cell_text.clear();
        }
        b"pic" => {
            state.in_picture = true;
            state.picture_alt = None;
            state.picture_embed_id = None;
        }
        b"cNvPr" if state.in_picture => {
            collect_string_attribute(event, b"descr", &mut state.picture_alt, budget)?;
        }
        b"blip" if state.in_picture => collect_embed_id(event, state, budget)?,
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
        _ => {}
    }
    Ok(())
}

fn finish_paragraph(state: &mut State, budget: &mut Budget<'_>) -> Result<()> {
    if !state.current_text.trim().is_empty() {
        budget.charge_item("PPTX paragraphs")?;
        state.current_paragraphs.push(PptxTextPara {
            text: std::mem::take(&mut state.current_text),
            list_level: state.current_list_level,
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
                    .join(" "),
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

fn collect_string_attribute(
    event: &BytesStart<'_>,
    key: &[u8],
    target: &mut Option<String>,
    budget: &mut Budget<'_>,
) -> Result<()> {
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid slide attribute")?;
        if attribute.key.as_ref() == key {
            budget.charge_output(
                attribute.value.len() as u64,
                "PPTX retained slide attributes",
            )?;
            *target = Some(String::from_utf8_lossy(&attribute.value).to_string());
        }
    }
    Ok(())
}

fn collect_embed_id(
    event: &BytesStart<'_>,
    state: &mut State,
    budget: &mut Budget<'_>,
) -> Result<()> {
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid image attribute")?;
        if local_name(attribute.key.as_ref()) == b"embed" {
            budget.charge_output(
                attribute.value.len() as u64,
                "PPTX retained image relationship IDs",
            )?;
            state.picture_embed_id = Some(String::from_utf8_lossy(&attribute.value).to_string());
        }
    }
    Ok(())
}

fn collect_list_level(event: &BytesStart<'_>, state: &mut State) -> Result<()> {
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid paragraph attribute")?;
        if attribute.key.as_ref() == b"lvl" {
            state.current_list_level = Some(
                String::from_utf8_lossy(&attribute.value)
                    .parse::<u32>()
                    .context("extract_pptx: paragraph list level must be an unsigned integer")?,
            );
        }
    }
    Ok(())
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}
