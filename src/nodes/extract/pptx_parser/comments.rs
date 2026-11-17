//! Slide comments. A slide's modern comments part (MS-PPTX) takes
//! precedence over its legacy `comments` part: PowerPoint can keep a legacy
//! copy of the same thread for older readers, and the two formats share no
//! identifier to deduplicate by, so the legacy part of such a slide is not read.

use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};

use super::PptxComment;
use super::authors::{AuthorList, Authors, assign_author, local_name, read_authors};
use super::graph::Presentation;
use super::modern_comments::parse_modern_comments;
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;
use crate::util::xml::{Decoder, attribute_value};

pub(in crate::nodes::extract) fn extract_pptx_comments(
    archive: &mut Archive,
    presentation: &Presentation,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<Vec<PptxComment>> {
    let legacy_authors = read_authors(
        archive,
        presentation.authors.as_deref(),
        AuthorList::Legacy,
        budget,
        execution,
    )?;
    let modern_authors = read_authors(
        archive,
        presentation.modern_authors.as_deref(),
        AuthorList::Modern,
        budget,
        execution,
    )?;

    let mut comments = Vec::new();
    for slide in &presentation.slides {
        budget.checkpoint()?;
        let parsed = if let Some(name) = &slide.modern_comments {
            archive.with_required_xml(name, execution, |reader| {
                parse_modern_comments(reader, slide.index, &modern_authors, budget)
            })?
        } else if let Some(name) = &slide.comments {
            archive.with_required_xml(name, execution, |reader| {
                parse_comments(reader, slide.index, &legacy_authors, budget)
            })?
        } else {
            continue;
        };
        comments.extend(parsed);
    }
    Ok(comments)
}

fn parse_comments<R: BufRead>(
    xml: R,
    slide_index: u32,
    authors: &Authors,
    budget: &mut Budget<'_>,
) -> Result<Vec<PptxComment>> {
    let mut comments = Vec::new();
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut decoder = Decoder::default();
    let mut buffer = Vec::new();
    let mut current = None;
    let mut in_text = false;
    let mut saw_element = false;
    let mut depth = 0_u64;
    loop {
        budget.checkpoint()?;
        match reader
            .read_event_into(&mut buffer)
            .map_err(anyhow::Error::from)
            .and_then(|event| {
                decoder.decode(event, |bytes| {
                    budget.charge_output(bytes, "PPTX decoded comment text")
                })
            }) {
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                budget.charge_item("PPTX comment XML events")?;
                match local_name(event.name().as_ref().as_bytes()) {
                    b"cm" => {
                        if current.is_some() {
                            anyhow::bail!("extract_pptx: nested comments are not supported");
                        }
                        current = Some(parse_comment(
                            &event,
                            slide_index,
                            authors,
                            decoder.version,
                            budget,
                        )?);
                    }
                    b"text" => in_text = current.is_some(),
                    _ => {}
                }
            }
            Ok(Event::Empty(event)) => {
                saw_element = true;
                budget.charge_item("PPTX comment XML events")?;
                if local_name(event.name().as_ref().as_bytes()) == b"cm" {
                    let comment =
                        parse_comment(&event, slide_index, authors, decoder.version, budget)?;
                    budget.charge_item("PPTX comments")?;
                    comments.push(comment);
                }
            }
            Ok(Event::Text(event)) if in_text => {
                budget.charge_item("PPTX comment XML events")?;
                if let Some(comment) = current.as_mut() {
                    comment.entry.text.push_str(event.as_ref());
                }
            }
            Ok(Event::End(event)) => {
                budget.charge_item("PPTX comment XML events")?;
                depth = depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_pptx: unmatched closing element in comments")
                })?;
                match local_name(event.name().as_ref().as_bytes()) {
                    b"text" => in_text = false,
                    b"cm" => {
                        if let Some(comment) = current.take() {
                            budget.charge_item("PPTX comments")?;
                            comments.push(comment);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => budget.charge_item("PPTX comment XML events")?,
            Err(error) => anyhow::bail!("extract_pptx: invalid XML in comments: {error}"),
        }
        buffer.clear();
    }
    if !saw_element || current.is_some() || depth != 0 {
        anyhow::bail!("extract_pptx: incomplete XML in comments part");
    }
    Ok(comments)
}

fn parse_comment(
    event: &BytesStart<'_>,
    slide_index: u32,
    authors: &Authors,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<PptxComment> {
    let mut comment = PptxComment {
        slide_index,
        ..Default::default()
    };
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid comment attribute")?;
        let value = attribute_value(&attribute, version, |bytes| {
            budget.charge_output(bytes, "PPTX comment attribute decoding")
        })?
        .into_owned();
        match attribute.key.as_ref().as_bytes() {
            b"authorId" => assign_author(&mut comment.entry, authors, value, budget)?,
            b"dt" => {
                budget.charge_output(value.len() as u64, "PPTX retained comment attributes")?;
                comment.entry.date = Some(value);
            }
            b"idx" => {
                budget.charge_output(value.len() as u64, "PPTX retained comment attributes")?;
                comment.idx = Some(value);
            }
            _ => {}
        }
    }
    Ok(comment)
}
