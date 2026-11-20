//! Bounded parsing of OPC relationship parts (`*.rels`) shared by the DOCX
//! and PPTX readers.

use std::collections::HashMap;
use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::NsReader;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;

use crate::nodes::extract::resource::Budget;
use crate::util::xml::{Decoder, attribute_value};

const OPC_RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// Wording and budget labels for one reader, so each node keeps its own
/// operation name and limit messages.
#[derive(Clone, Copy)]
pub(in crate::nodes::extract) struct Labels {
    pub(in crate::nodes::extract) operation: &'static str,
    /// Noun used in parse errors, e.g. "slide relationship".
    pub(in crate::nodes::extract) noun: &'static str,
    pub(in crate::nodes::extract) events: &'static str,
    pub(in crate::nodes::extract) text: &'static str,
    pub(in crate::nodes::extract) relationships: &'static str,
    pub(in crate::nodes::extract) attributes: &'static str,
    pub(in crate::nodes::extract) retained: &'static str,
}

pub(in crate::nodes::extract) const PPTX: Labels = Labels {
    operation: "extract_pptx",
    noun: "slide relationship",
    events: "PPTX relationship XML events",
    text: "PPTX relationship XML text",
    relationships: "PPTX relationships",
    attributes: "PPTX relationship attributes",
    retained: "PPTX retained relationship attributes",
};

pub(in crate::nodes::extract) const DOCX: Labels = Labels {
    operation: "extract_word",
    noun: "relationship",
    events: "DOCX relationship XML events",
    text: "DOCX relationship XML text",
    relationships: "DOCX relationships",
    attributes: "DOCX relationship attributes",
    retained: "DOCX retained relationship attributes",
};

pub(in crate::nodes::extract) struct Relationship<K> {
    pub(in crate::nodes::extract) target: String,
    pub(in crate::nodes::extract) kind: K,
    pub(in crate::nodes::extract) external: bool,
}

pub(in crate::nodes::extract) type Relationships<K> = HashMap<String, Relationship<K>>;

/// Parses one relationship part. `classify` maps the relationship `Type` URI
/// to the reader's own kind, so unknown types are not retained as strings.
pub(in crate::nodes::extract) fn parse_relationships<R: BufRead, K>(
    xml: R,
    labels: &Labels,
    classify: fn(&str) -> K,
    budget: &mut Budget<'_>,
) -> Result<Relationships<K>> {
    let operation = labels.operation;
    let mut reader = NsReader::from_reader(xml);
    let mut decoder = Decoder::default();
    let mut buffer = Vec::new();
    let mut relationships = HashMap::new();
    let mut depth = 0_usize;
    let mut root_seen = false;
    loop {
        budget.charge_item(labels.events)?;
        let event = reader.read_event_into(&mut buffer).map_err(|error| {
            anyhow::anyhow!("{operation}: invalid XML in {}s: {error}", labels.noun)
        })?;
        let event = decoder.decode(event, |bytes| budget.charge_output(bytes, labels.text))?;
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                let (namespace, name) = reader.resolver().resolve_element(start.name());
                // Retain compatibility with legacy unqualified relationship XML,
                // but never reinterpret a bound foreign namespace as OPC.
                anyhow::ensure!(
                    matches!(namespace, ResolveResult::Unbound)
                        || matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == OPC_RELATIONSHIPS),
                    "{operation}: invalid relationship namespace"
                );
                if depth == 0 {
                    anyhow::ensure!(
                        !root_seen && name.as_ref() == "Relationships",
                        "{operation}: invalid relationship root"
                    );
                    root_seen = true;
                } else {
                    anyhow::ensure!(
                        depth == 1 && name.as_ref() == "Relationship",
                        "{operation}: invalid nested relationship element"
                    );
                    budget.charge_item(labels.relationships)?;
                    let (id, relationship) =
                        parse_relationship(start, decoder.version, labels, classify, budget)?;
                    anyhow::ensure!(
                        !relationships.contains_key(&id),
                        "{operation}: duplicate {} Id '{id}'",
                        labels.noun
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
                    .with_context(|| format!("{operation}: unmatched relationship end"))?;
            }
            Event::Text(text) => anyhow::ensure!(
                text.trim().is_empty(),
                "{operation}: unexpected relationship text"
            ),
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    anyhow::ensure!(
        root_seen && depth == 0,
        "{operation}: incomplete XML in {}s",
        labels.noun
    );
    Ok(relationships)
}

fn parse_relationship<K>(
    event: &BytesStart<'_>,
    version: quick_xml::XmlVersion,
    labels: &Labels,
    classify: fn(&str) -> K,
    budget: &mut Budget<'_>,
) -> Result<(String, Relationship<K>)> {
    let operation = labels.operation;
    let mut id = None;
    let mut target = None;
    let mut relationship_type = None;
    let mut external = false;
    for attribute in event.attributes() {
        budget.charge_item(labels.attributes)?;
        let attribute =
            attribute.with_context(|| format!("{operation}: invalid relationship attribute"))?;
        if !matches!(
            attribute.key.as_ref(),
            "Id" | "Target" | "Type" | "TargetMode"
        ) {
            continue;
        }
        let value = attribute_value(&attribute, version, |bytes| {
            budget.charge_output(bytes, labels.retained)
        })
        .map_err(|error| {
            let message = format!("{operation}: invalid relationship attribute value: {error:#}");
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
                anyhow::bail!("{operation}: invalid relationship TargetMode '{value}'")
            }
            _ => {}
        }
    }
    Ok((
        required_attribute(id, "Id", operation)?,
        Relationship {
            target: required_attribute(target, "Target", operation)?,
            kind: classify(&required_attribute(relationship_type, "Type", operation)?),
            external,
        },
    ))
}

fn required_attribute(value: Option<String>, name: &str, operation: &str) -> Result<String> {
    match value {
        Some(value) if !value.is_empty() => Ok(value),
        _ => anyhow::bail!("{operation}: Relationship is missing required {name} attribute"),
    }
}

/// Returns the suffix of an officeDocument relationship type in either the
/// transitional or the strict namespace.
pub(in crate::nodes::extract) fn office_document_type(value: &str) -> Option<&str> {
    value
        .strip_prefix("http://schemas.openxmlformats.org/officeDocument/2006/relationships/")
        .or_else(|| value.strip_prefix("http://purl.oclc.org/ooxml/officeDocument/relationships/"))
}
