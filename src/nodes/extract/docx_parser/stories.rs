//! One event walk for every WordprocessingML story: the main document,
//! headers and footers, and the footnotes and endnotes parts.

use std::collections::HashMap;
use std::io::BufRead;

use anyhow::Result;
use quick_xml::events::Event;

use super::anchors::AnchorCollector;
use super::blocks::{BlockBuilder, Lookups};
use super::xml::{XmlDocument, attribute_value};
use super::{DocxBlock, NoteKind};
use crate::nodes::extract::resource::Budget;

pub(super) fn walk<R: BufRead>(
    xml: R,
    part: &'static str,
    builder: &mut BlockBuilder,
    mut anchors: Option<&mut AnchorCollector<'_, '_>>,
    lookups: &Lookups<'_>,
    budget: &mut Budget<'_>,
    mut boundary: impl FnMut(
        &Event<'_>,
        &mut BlockBuilder,
        quick_xml::XmlVersion,
        &mut Budget<'_>,
    ) -> Result<()>,
) -> Result<()> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().check_comments = true;
    let mut buf = Vec::new();
    let mut document = XmlDocument::new(part);
    loop {
        budget.charge_item("DOCX XML events")?;
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|error| anyhow::anyhow!("extract_word: invalid {part}: {error}"))?;
        let event = document.decode(event, budget)?;
        let version = document.version();
        if let Some(anchors) = anchors.as_mut() {
            anchors.observe(&event, version, budget)?;
        }
        boundary(&event, builder, version, budget)?;
        match event {
            Event::Start(ref event) => builder.open(event, false, version, lookups, budget)?,
            Event::Empty(ref event) => builder.open(event, true, version, lookups, budget)?,
            Event::Text(ref event) => builder.text(event.as_ref()),
            Event::End(ref event) => builder.close(event.name().as_ref(), lookups.styles),
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(())
}

/// Blocks of a header or footer part (`w:hdr` / `w:ftr`).
pub(in crate::nodes::extract) fn parse_docx_story<R: BufRead>(
    xml: R,
    part: &'static str,
    lookups: &Lookups<'_>,
    budget: &mut Budget<'_>,
) -> Result<Vec<DocxBlock>> {
    let mut builder = BlockBuilder::default();
    walk(
        xml,
        part,
        &mut builder,
        None,
        lookups,
        budget,
        |_, _, _, _| Ok(()),
    )?;
    Ok(builder.finish().0)
}

/// Blocks of the referenced notes in a footnotes or endnotes part, indexed
/// like `wanted` (note ids in label order). Notes nobody references, such as
/// the separator notes, are dropped as soon as they end, and a missing note
/// stays `None`. The first definition of a duplicated id wins.
pub(in crate::nodes::extract) fn parse_docx_notes<R: BufRead>(
    xml: R,
    part: &'static str,
    kind: NoteKind,
    wanted: &[String],
    lookups: &Lookups<'_>,
    budget: &mut Budget<'_>,
) -> Result<Vec<Option<Vec<DocxBlock>>>> {
    let element = match kind {
        NoteKind::Footnote => "w:footnote",
        NoteKind::Endnote => "w:endnote",
    };
    let positions: HashMap<&str, usize> = wanted
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let mut notes: Vec<Option<Vec<DocxBlock>>> = vec![None; wanted.len()];
    let mut current: Option<Option<usize>> = None;
    let mut builder = BlockBuilder::default();
    let mut boundary = |event: &Event<'_>,
                        builder: &mut BlockBuilder,
                        version: quick_xml::XmlVersion,
                        budget: &mut Budget<'_>|
     -> Result<()> {
        match event {
            Event::Start(start) if start.name().as_ref() == element && current.is_none() => {
                let id = attribute_value(start, b"w:id", part, version, budget)?;
                current = Some(id.and_then(|id| positions.get(id.as_str()).copied()));
                builder.take_blocks();
            }
            Event::End(end) if end.name().as_ref() == element => {
                // Blocks are finished on their own end tags, before this one.
                let blocks = builder.take_blocks();
                if let Some(Some(index)) = current.take()
                    && notes[index].is_none()
                {
                    notes[index] = Some(blocks);
                }
            }
            _ => {}
        }
        Ok(())
    };
    walk(
        xml,
        part,
        &mut builder,
        None,
        lookups,
        budget,
        &mut boundary,
    )?;
    Ok(notes)
}
