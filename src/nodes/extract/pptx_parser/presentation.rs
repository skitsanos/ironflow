use std::collections::HashSet;
use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::NsReader;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{NamespaceResolver, ResolveResult};

use crate::nodes::extract::resource::Budget;
use crate::util::xml::{Decoder, attribute_value};

pub(super) fn slide_ids<R: BufRead>(xml: R, budget: &mut Budget<'_>) -> Result<Vec<String>> {
    let mut reader = NsReader::from_reader(xml);
    let mut decoder = Decoder::default();
    let mut buffer = Vec::new();
    let mut depth = 0_usize;
    let mut root_seen = false;
    let mut list_seen = false;
    let mut in_list = false;
    let mut numeric_ids = HashSet::new();
    let mut relationship_ids = HashSet::new();
    let mut slides = Vec::new();
    loop {
        budget.charge_item("PPTX presentation XML events")?;
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|error| anyhow::anyhow!("extract_pptx: invalid presentation XML: {error}"))?;
        let event = decoder.decode(event, |bytes| {
            budget.charge_output(bytes, "PPTX presentation XML text")
        })?;
        match event {
            Event::Start(ref element) | Event::Empty(ref element) => {
                anyhow::ensure!(
                    depth < 128,
                    "extract_pptx: presentation XML nesting exceeds 128"
                );
                let (namespace, local) = reader.resolver().resolve_element(element.name());
                let pml = is_presentation_namespace(namespace);
                if depth == 0 {
                    anyhow::ensure!(
                        !root_seen && pml && local.as_ref() == "presentation",
                        "extract_pptx: invalid presentation root"
                    );
                    root_seen = true;
                } else if depth == 1 && pml && local.as_ref() == "sldIdLst" {
                    anyhow::ensure!(
                        !list_seen,
                        "extract_pptx: duplicate presentation slide list"
                    );
                    list_seen = true;
                    in_list = matches!(event, Event::Start(_));
                } else if in_list && depth == 2 {
                    anyhow::ensure!(
                        pml && local.as_ref() == "sldId",
                        "extract_pptx: invalid slide list member"
                    );
                    let (id, relationship) =
                        slide_id(element, reader.resolver(), decoder.version, budget)?;
                    anyhow::ensure!(
                        numeric_ids.insert(id),
                        "extract_pptx: duplicate presentation slide id"
                    );
                    budget.charge_output(
                        relationship.len() as u64,
                        "PPTX slide reference identity copy",
                    )?;
                    anyhow::ensure!(
                        relationship_ids.insert(relationship.clone()),
                        "extract_pptx: duplicate presentation slide relationship"
                    );
                    slides.push(relationship);
                } else if in_list {
                    anyhow::bail!("extract_pptx: invalid nested slide list member");
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .context("extract_pptx: unmatched presentation end")?;
                if depth == 1 {
                    in_list = false;
                }
            }
            Event::Text(text) => anyhow::ensure!(
                text.trim().is_empty(),
                "extract_pptx: unexpected presentation text"
            ),
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    anyhow::ensure!(
        root_seen && depth == 0,
        "extract_pptx: incomplete presentation XML"
    );
    anyhow::ensure!(
        !slides.is_empty(),
        "extract_pptx: presentation contains no slides"
    );
    Ok(slides)
}

fn slide_id(
    element: &BytesStart<'_>,
    resolver: &NamespaceResolver,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<(u32, String)> {
    let mut id = None;
    let mut relationship = None;
    for attribute in element.attributes() {
        budget.charge_item("PPTX slide reference attributes")?;
        let attribute = attribute.context("extract_pptx: invalid slide reference attribute")?;
        let (namespace, name) = resolver.resolve_attribute(attribute.key);
        if name.as_ref() != "id" {
            continue;
        }
        let value = attribute_value(&attribute, version, |bytes| {
            budget.charge_output(bytes, "PPTX retained slide references")
        })?;
        match namespace {
            ResolveResult::Unbound => {
                let value = value
                    .parse::<u32>()
                    .context("extract_pptx: slide id must be an unsigned integer")?;
                anyhow::ensure!(
                    value != 0 && id.replace(value).is_none(),
                    "extract_pptx: invalid or duplicate slide id attribute"
                );
            }
            ResolveResult::Bound(ns)
                if matches!(
                    ns.as_ref(),
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                        | "http://purl.oclc.org/ooxml/officeDocument/relationships"
                ) =>
            {
                anyhow::ensure!(
                    !value.is_empty() && relationship.replace(value.into_owned()).is_none(),
                    "extract_pptx: invalid or duplicate slide relationship attribute"
                );
            }
            _ => anyhow::bail!("extract_pptx: invalid slide relationship namespace"),
        }
    }
    Ok((
        id.context("extract_pptx: missing slide id")?,
        relationship.context("extract_pptx: missing slide relationship id")?,
    ))
}

fn is_presentation_namespace(namespace: ResolveResult<'_>) -> bool {
    matches!(namespace, ResolveResult::Bound(ns) if matches!(ns.as_ref(),
        "http://schemas.openxmlformats.org/presentationml/2006/main" |
        "http://purl.oclc.org/ooxml/presentationml/main"))
}
