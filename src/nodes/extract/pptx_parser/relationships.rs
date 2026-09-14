use std::collections::HashMap;
use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::NsReader;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;

use crate::nodes::extract::resource::Budget;
use crate::util::xml::{Decoder, attribute_value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Slide,
    Notes,
    Comments,
    Authors,
    Image,
    Other,
}

pub(super) struct Relationship {
    pub(super) target: String,
    pub(super) kind: Kind,
    pub(super) external: bool,
}

pub(super) type Relationships = HashMap<String, Relationship>;

pub(super) fn parse_pptx_rels<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<Relationships> {
    let mut reader = NsReader::from_reader(xml);
    let mut decoder = Decoder::default();
    let mut buffer = Vec::new();
    let mut relationships = HashMap::new();
    let mut depth = 0_usize;
    let mut root_seen = false;
    loop {
        budget.charge_item("PPTX relationship XML events")?;
        let event = reader.read_event_into(&mut buffer).map_err(|error| {
            anyhow::anyhow!("extract_pptx: invalid XML in slide relationships: {error}")
        })?;
        let event = decoder.decode(event, |bytes| {
            budget.charge_output(bytes, "PPTX relationship XML text")
        })?;
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                let (namespace, name) = reader.resolver().resolve_element(start.name());
                // Retain compatibility with legacy unqualified relationship XML,
                // but never reinterpret a bound foreign namespace as OPC.
                anyhow::ensure!(
                    matches!(namespace, ResolveResult::Unbound)
                        || matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == "http://schemas.openxmlformats.org/package/2006/relationships"),
                    "extract_pptx: invalid relationship namespace"
                );
                if depth == 0 {
                    anyhow::ensure!(
                        !root_seen && name.as_ref() == "Relationships",
                        "extract_pptx: invalid relationship root"
                    );
                    root_seen = true;
                } else {
                    anyhow::ensure!(
                        depth == 1 && name.as_ref() == "Relationship",
                        "extract_pptx: invalid nested relationship element"
                    );
                    budget.charge_item("PPTX relationships")?;
                    let (id, relationship) = parse_relationship(start, decoder.version, budget)?;
                    anyhow::ensure!(
                        !relationships.contains_key(&id),
                        "extract_pptx: duplicate slide relationship Id '{id}'"
                    );
                    relationships.insert(id, relationship);
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .context("extract_pptx: unmatched relationship end")?;
            }
            Event::Text(text) => anyhow::ensure!(
                text.trim().is_empty(),
                "extract_pptx: unexpected relationship text"
            ),
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    anyhow::ensure!(
        root_seen && depth == 0,
        "extract_pptx: incomplete XML in slide relationships"
    );
    Ok(relationships)
}

fn parse_relationship(
    event: &BytesStart<'_>,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<(String, Relationship)> {
    let mut id = None;
    let mut target = None;
    let mut relationship_type = None;
    let mut external = false;
    for attribute in event.attributes() {
        budget.charge_item("PPTX relationship attributes")?;
        let attribute = attribute.context("extract_pptx: invalid relationship attribute")?;
        if !matches!(
            attribute.key.as_ref(),
            "Id" | "Target" | "Type" | "TargetMode"
        ) {
            continue;
        }
        let value = attribute_value(&attribute, version, |bytes| {
            budget.charge_output(bytes, "PPTX retained relationship attributes")
        })
        .map_err(|error| {
            let message = format!("extract_pptx: invalid relationship attribute value: {error:#}");
            error.context(message)
        })?
        .into_owned();
        match attribute.key.as_ref() {
            "Id" => id = Some(value),
            "Target" => target = Some(value),
            "Type" => relationship_type = Some(value),
            "TargetMode" if value.eq_ignore_ascii_case("internal") => external = false,
            "TargetMode" if value.eq_ignore_ascii_case("external") => external = true,
            "TargetMode" => {
                anyhow::bail!("extract_pptx: invalid relationship TargetMode '{value}'")
            }
            _ => {}
        }
    }
    Ok((
        required_attribute(id, "Id")?,
        Relationship {
            target: required_attribute(target, "Target")?,
            kind: relationship_kind(&required_attribute(relationship_type, "Type")?),
            external,
        },
    ))
}

fn required_attribute(value: Option<String>, name: &str) -> Result<String> {
    match value {
        Some(value) if !value.is_empty() => Ok(value),
        _ => anyhow::bail!("extract_pptx: Relationship is missing required {name} attribute"),
    }
}

fn relationship_kind(value: &str) -> Kind {
    let suffix = value
        .strip_prefix("http://schemas.openxmlformats.org/officeDocument/2006/relationships/")
        .or_else(|| value.strip_prefix("http://purl.oclc.org/ooxml/officeDocument/relationships/"));
    match suffix {
        Some("slide") => Kind::Slide,
        Some("notesSlide") => Kind::Notes,
        Some("comments") => Kind::Comments,
        Some("commentAuthors") => Kind::Authors,
        Some("image") => Kind::Image,
        _ => Kind::Other,
    }
}
