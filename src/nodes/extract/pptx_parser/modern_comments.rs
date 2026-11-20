//! PowerPoint modern comments (MS-PPTX 2.16.3, `p188:cmLst`). A `cm` carries
//! `id`, `authorId`, `created` and optional `status`; its text is the
//! paragraphs of `p188:txBody`, and `p188:replyLst/p188:reply` holds replies
//! with their own attributes and text body. Anchor monikers and extension
//! lists carry no text and are skipped.

use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};

use super::authors::{Authors, assign_author, local_name};
use super::{PptxComment, PptxCommentEntry};
use crate::nodes::extract::resource::Budget;
use crate::util::xml::{Decoder, attribute_value};

const EVENTS: &str = "PPTX modern comment XML events";

#[derive(Default)]
struct State {
    comments: Vec<PptxComment>,
    comment: Option<PptxComment>,
    reply: Option<PptxCommentEntry>,
    in_body: bool,
    paragraphs: u32,
    in_text: bool,
    depth: u64,
}

impl State {
    fn text(&mut self) -> Option<&mut String> {
        match (&mut self.reply, &mut self.comment) {
            (Some(reply), _) => Some(&mut reply.text),
            (None, Some(comment)) => Some(&mut comment.entry.text),
            (None, None) => None,
        }
    }

    fn newline(&mut self, budget: &mut Budget<'_>) -> Result<()> {
        if let Some(text) = self.text() {
            budget.charge_output(1, "PPTX comment line breaks")?;
            text.push('\n');
        }
        Ok(())
    }

    fn open(
        &mut self,
        event: &BytesStart<'_>,
        empty: bool,
        slide_index: u32,
        authors: &Authors,
        version: quick_xml::XmlVersion,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        let qualified = event.name();
        let name = local_name(qualified.as_ref().as_bytes());
        if self.depth == 0 {
            anyhow::ensure!(
                name == b"cmLst",
                "extract_pptx: modern comments part must hold a modern comment list (cmLst)"
            );
        }
        match name {
            b"cm" => {
                anyhow::ensure!(
                    self.comment.is_none(),
                    "extract_pptx: nested comments are not supported"
                );
                let entry = parse_entry(event, authors, version, budget)?;
                self.comment = Some(PptxComment {
                    slide_index,
                    entry,
                    ..Default::default()
                });
                if empty {
                    self.close(b"cm", budget)?;
                }
            }
            b"reply" => {
                anyhow::ensure!(
                    self.comment.is_some() && self.reply.is_none(),
                    "extract_pptx: modern comment reply outside a comment"
                );
                self.reply = Some(parse_entry(event, authors, version, budget)?);
                if empty {
                    self.close(b"reply", budget)?;
                }
            }
            b"txBody" if !empty && self.comment.is_some() => {
                self.in_body = true;
                self.paragraphs = 0;
            }
            b"p" if self.in_body => {
                if self.paragraphs > 0 {
                    self.newline(budget)?;
                }
                self.paragraphs = self.paragraphs.saturating_add(1);
            }
            b"br" if self.in_body => self.newline(budget)?,
            b"t" if self.in_body && !empty => self.in_text = true,
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self, name: &[u8], budget: &mut Budget<'_>) -> Result<()> {
        match name {
            b"t" => self.in_text = false,
            b"txBody" => self.in_body = false,
            b"reply" => {
                if let (Some(reply), Some(comment)) = (self.reply.take(), self.comment.as_mut()) {
                    budget.charge_item("PPTX comment replies")?;
                    comment.replies.push(reply);
                }
            }
            b"cm" => {
                if let Some(comment) = self.comment.take() {
                    budget.charge_item("PPTX comments")?;
                    self.comments.push(comment);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

pub(super) fn parse_modern_comments<R: BufRead>(
    xml: R,
    slide_index: u32,
    authors: &Authors,
    budget: &mut Budget<'_>,
) -> Result<Vec<PptxComment>> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut decoder = Decoder::default();
    let mut buffer = Vec::new();
    let mut state = State::default();
    let mut saw_element = false;
    loop {
        budget.checkpoint()?;
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(anyhow::Error::from)
            .and_then(|event| {
                decoder.decode(event, |bytes| {
                    budget.charge_output(bytes, "PPTX decoded comment text")
                })
            })
            .map_err(|error| anyhow::anyhow!("extract_pptx: invalid XML in comments: {error}"))?;
        budget.charge_item(EVENTS)?;
        match event {
            Event::Start(event) => {
                saw_element = true;
                let version = decoder.version;
                state.open(&event, false, slide_index, authors, version, budget)?;
                state.depth = state.depth.saturating_add(1);
            }
            Event::Empty(event) => {
                saw_element = true;
                let version = decoder.version;
                state.open(&event, true, slide_index, authors, version, budget)?;
            }
            Event::Text(event) if state.in_text => {
                if let Some(text) = state.text() {
                    text.push_str(event.as_ref());
                }
            }
            Event::End(event) => {
                state.depth = state.depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_pptx: unmatched closing element in comments")
                })?;
                let qualified = event.name();
                state.close(local_name(qualified.as_ref().as_bytes()), budget)?;
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if !saw_element || state.comment.is_some() || state.depth != 0 {
        anyhow::bail!("extract_pptx: incomplete XML in comments part");
    }
    Ok(state.comments)
}

fn parse_entry(
    event: &BytesStart<'_>,
    authors: &Authors,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<PptxCommentEntry> {
    let mut entry = PptxCommentEntry::default();
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid comment attribute")?;
        let value = attribute_value(&attribute, version, |bytes| {
            budget.charge_output(bytes, "PPTX comment attribute decoding")
        })?
        .into_owned();
        let slot = match attribute.key.as_ref().as_bytes() {
            b"authorId" => {
                assign_author(&mut entry, authors, value, budget)?;
                continue;
            }
            b"id" => &mut entry.id,
            b"created" => &mut entry.date,
            b"status" => &mut entry.status,
            _ => continue,
        };
        budget.charge_output(value.len() as u64, "PPTX retained comment attributes")?;
        *slot = Some(value);
    }
    Ok(entry)
}
