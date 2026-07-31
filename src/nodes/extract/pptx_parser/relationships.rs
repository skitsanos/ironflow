use std::collections::{HashMap, HashSet};
use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::nodes::extract::resource::Budget;

pub(super) fn parse_pptx_rels<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<HashMap<String, String>> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut relationships = HashMap::new();
    let mut ignored_relationship_ids = HashSet::new();
    let mut saw_element = false;
    let mut depth = 0_u64;
    let mut xml_version = XmlVersion::Implicit1_0;
    loop {
        budget.checkpoint()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Decl(declaration)) => {
                budget.charge_item("PPTX relationship XML events")?;
                xml_version = declaration
                    .xml_version()
                    .context("extract_pptx: invalid relationship XML declaration")?;
            }
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                budget.charge_item("PPTX relationship XML events")?;
                collect_relationship(
                    &event,
                    xml_version,
                    &mut ignored_relationship_ids,
                    &mut relationships,
                    budget,
                )?;
            }
            Ok(Event::Empty(event)) => {
                saw_element = true;
                budget.charge_item("PPTX relationship XML events")?;
                collect_relationship(
                    &event,
                    xml_version,
                    &mut ignored_relationship_ids,
                    &mut relationships,
                    budget,
                )?;
            }
            Ok(Event::End(_)) => {
                budget.charge_item("PPTX relationship XML events")?;
                depth = depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!(
                        "extract_pptx: unmatched closing element in slide relationships"
                    )
                })?;
            }
            Ok(Event::Eof) => break,
            Ok(_) => budget.charge_item("PPTX relationship XML events")?,
            Err(error) => {
                anyhow::bail!("extract_pptx: invalid XML in slide relationships: {error}")
            }
        }
        buffer.clear();
    }
    if !saw_element || depth != 0 {
        anyhow::bail!("extract_pptx: incomplete XML in slide relationships");
    }
    Ok(relationships)
}

fn collect_relationship(
    event: &BytesStart<'_>,
    xml_version: XmlVersion,
    ignored_relationship_ids: &mut HashSet<String>,
    relationships: &mut HashMap<String, String>,
    budget: &mut Budget<'_>,
) -> Result<()> {
    if local_name(event.name().as_ref()) != b"Relationship" {
        return Ok(());
    }

    budget.charge_item("PPTX relationships")?;
    let relationship = parse_relationship(event, xml_version)?;
    budget.charge_output(
        relationship.id.len() as u64,
        "PPTX retained relationship IDs",
    )?;
    if ignored_relationship_ids.contains(&relationship.id)
        || relationships.contains_key(&relationship.id)
    {
        anyhow::bail!(
            "extract_pptx: duplicate slide relationship Id '{}'",
            relationship.id
        );
    }
    if relationship.target_mode == TargetMode::External
        || !is_image_relationship_type(&relationship.relationship_type)
    {
        ignored_relationship_ids.insert(relationship.id);
        return Ok(());
    }
    budget.charge_output(
        relationship.target.len() as u64,
        "PPTX retained relationship targets",
    )?;
    relationships.insert(relationship.id, relationship.target);
    Ok(())
}

struct Relationship {
    id: String,
    target: String,
    relationship_type: String,
    target_mode: TargetMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetMode {
    Internal,
    External,
}

fn parse_relationship(event: &BytesStart<'_>, xml_version: XmlVersion) -> Result<Relationship> {
    let mut id = None;
    let mut target = None;
    let mut relationship_type = None;
    let mut target_mode = TargetMode::Internal;
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid relationship attribute")?;
        if !matches!(
            attribute.key.as_ref(),
            b"Id" | b"Target" | b"Type" | b"TargetMode"
        ) {
            continue;
        }
        let value = attribute
            .decoded_and_normalized_value(xml_version, event.decoder())
            .context("extract_pptx: invalid relationship attribute value")?
            .into_owned();
        match attribute.key.as_ref() {
            b"Id" => id = Some(value),
            b"Target" => target = Some(value),
            b"Type" => relationship_type = Some(value),
            b"TargetMode" if value.eq_ignore_ascii_case("internal") => {
                target_mode = TargetMode::Internal;
            }
            b"TargetMode" if value.eq_ignore_ascii_case("external") => {
                target_mode = TargetMode::External;
            }
            b"TargetMode" => {
                anyhow::bail!("extract_pptx: invalid relationship TargetMode '{value}'")
            }
            _ => {}
        }
    }
    Ok(Relationship {
        id: required_attribute(id, "Id")?,
        target: required_attribute(target, "Target")?,
        relationship_type: required_attribute(relationship_type, "Type")?,
        target_mode,
    })
}

fn required_attribute(value: Option<String>, name: &str) -> Result<String> {
    match value {
        Some(value) if !value.is_empty() => Ok(value),
        _ => anyhow::bail!("extract_pptx: Relationship is missing required {name} attribute"),
    }
}

fn is_image_relationship_type(relationship_type: &str) -> bool {
    matches!(
        relationship_type,
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"
            | "http://purl.oclc.org/ooxml/officeDocument/relationships/image"
    )
}

pub(super) fn normalize_pptx_path(source_dir: &str, target: &str) -> Result<String> {
    if target.is_empty() {
        anyhow::bail!("extract_pptx: empty image relationship target");
    }
    if target.starts_with('/') || target.contains('\\') {
        anyhow::bail!("extract_pptx: invalid absolute image relationship target: {target}");
    }
    if target.contains('?') || target.contains('#') {
        anyhow::bail!("extract_pptx: image relationship target has a query or fragment: {target}");
    }
    let first_segment = target.split('/').next().unwrap_or_default();
    if first_segment.contains(':') {
        anyhow::bail!("extract_pptx: image relationship target has a URI scheme: {target}");
    }
    if target.split('/').any(str::is_empty) {
        anyhow::bail!("extract_pptx: image relationship target has an empty segment: {target}");
    }
    if matches!(target.rsplit('/').next(), Some("." | "..")) {
        anyhow::bail!("extract_pptx: image relationship target names a directory: {target}");
    }
    let mut parts: Vec<&str> = source_dir
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    anyhow::bail!(
                        "extract_pptx: image relationship escapes the package root: {target}"
                    );
                }
            }
            segment => parts.push(segment),
        }
    }
    Ok(parts.join("/"))
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}
