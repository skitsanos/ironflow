//! Comment-author lists: legacy `p:cmAuthorLst` (`commentAuthors`) and
//! modern `p188:authorLst` (MS-PPTX 2.16.2). Both map an author id to a
//! display name and initials.

use std::collections::HashMap;
use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};

use super::PptxCommentEntry;
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;
use crate::util::xml::{Decoder, attribute_value};

type Author = (Option<String>, Option<String>);

pub(super) type Authors = HashMap<String, Author>;

/// Which author list a part holds; selects the author element's local name.
#[derive(Clone, Copy)]
pub(super) enum AuthorList {
    Legacy,
    Modern,
}

impl AuthorList {
    fn element(self) -> &'static [u8] {
        match self {
            Self::Legacy => b"cmAuthor",
            Self::Modern => b"author",
        }
    }
}

pub(super) fn read_authors(
    archive: &mut Archive,
    name: Option<&str>,
    list: AuthorList,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<Authors> {
    Ok(name
        .map(|name| {
            archive.with_required_xml(name, execution, |reader| {
                parse_authors(reader, list, budget)
            })
        })
        .transpose()?
        .unwrap_or_default())
}

fn parse_authors<R: BufRead>(xml: R, list: AuthorList, budget: &mut Budget<'_>) -> Result<Authors> {
    let mut authors = HashMap::new();
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut decoder = Decoder::default();
    let mut buffer = Vec::new();
    let mut saw_element = false;
    let mut depth = 0_u64;
    loop {
        budget.checkpoint()?;
        match reader
            .read_event_into(&mut buffer)
            .map_err(anyhow::Error::from)
            .and_then(|event| {
                decoder.decode(event, |bytes| {
                    budget.charge_output(bytes, "PPTX decoded author text")
                })
            }) {
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                insert_author(&mut authors, &event, list, decoder.version, budget)?;
            }
            Ok(Event::Empty(event)) => {
                saw_element = true;
                insert_author(&mut authors, &event, list, decoder.version, budget)?;
            }
            Ok(Event::End(_)) => {
                budget.charge_item("PPTX comment-author XML events")?;
                depth = depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_pptx: unmatched closing element in comment authors")
                })?;
            }
            Ok(Event::Eof) => break,
            Ok(_) => budget.charge_item("PPTX comment-author XML events")?,
            Err(error) => {
                anyhow::bail!("extract_pptx: invalid XML in comment authors: {error}")
            }
        }
        buffer.clear();
    }
    if !saw_element || depth != 0 {
        anyhow::bail!("extract_pptx: incomplete XML in comment authors part");
    }
    Ok(authors)
}

fn insert_author(
    authors: &mut Authors,
    event: &BytesStart<'_>,
    list: AuthorList,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<()> {
    budget.charge_item("PPTX comment-author XML events")?;
    if local_name(event.name().as_ref().as_bytes()) == list.element()
        && let Some((id, author)) = parse_author(event, version, budget)?
    {
        budget.charge_item("PPTX comment authors")?;
        authors.insert(id, author);
    }
    Ok(())
}

fn parse_author(
    event: &BytesStart<'_>,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<Option<(String, Author)>> {
    let mut id = None;
    let mut name = None;
    let mut initials = None;
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid comment-author attribute")?;
        let value = attribute_value(&attribute, version, |bytes| {
            budget.charge_output(bytes, "PPTX author attribute decoding")
        })?
        .into_owned();
        match attribute.key.as_ref().as_bytes() {
            b"id" => id = Some(value),
            b"name" => name = Some(value),
            b"initials" => initials = Some(value),
            _ => {}
        }
    }
    let Some(id) = id.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    Ok(Some((id, (name, initials))))
}

/// Record `author_id` and, when the id is listed, the author's name and
/// initials. An unknown id keeps only `author_id`.
pub(super) fn assign_author(
    entry: &mut PptxCommentEntry,
    authors: &Authors,
    value: String,
    budget: &mut Budget<'_>,
) -> Result<()> {
    if let Some((name, initials)) = authors.get(&value) {
        if let Some(name) = name {
            budget.charge_output(name.len() as u64, "PPTX retained comment authors")?;
        }
        if let Some(initials) = initials {
            budget.charge_output(initials.len() as u64, "PPTX retained comment initials")?;
        }
        entry.author = name.clone();
        entry.initials = initials.clone();
    }
    budget.charge_output(value.len() as u64, "PPTX retained comment attributes")?;
    entry.author_id = Some(value);
    Ok(())
}

pub(super) fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}
