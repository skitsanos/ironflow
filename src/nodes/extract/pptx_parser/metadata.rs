use std::collections::BTreeMap;
use std::io::BufRead;

use anyhow::Result;
use quick_xml::events::Event;

use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

pub(in crate::nodes::extract) fn extract_pptx_metadata(
    archive: &mut Archive,
    slide_count: usize,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<BTreeMap<String, serde_json::Value>> {
    let mut metadata = BTreeMap::new();
    metadata.insert("slide_count".to_string(), serde_json::json!(slide_count));
    let properties = archive.with_optional_xml("docProps/core.xml", execution, |xml| {
        parse_core_properties(xml, budget)
    })?;
    if let Some(properties) = properties {
        metadata.extend(properties);
    }
    Ok(metadata)
}

fn parse_core_properties<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<BTreeMap<String, serde_json::Value>> {
    let mut metadata = BTreeMap::new();
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut current = None;
    let mut saw_element = false;
    let mut depth = 0_u64;
    loop {
        budget.checkpoint()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                budget.charge_item("PPTX metadata XML events")?;
                current = metadata_key(event.name().as_ref());
            }
            Ok(Event::Empty(_)) => {
                saw_element = true;
                budget.charge_item("PPTX metadata XML events")?;
            }
            Ok(Event::Text(event)) => {
                budget.charge_item("PPTX metadata XML events")?;
                if let Some(key) = current {
                    let value = String::from_utf8_lossy(event.as_ref()).trim().to_string();
                    if !value.is_empty() {
                        budget.charge_item("PPTX metadata fields")?;
                        budget
                            .charge_output(value.len() as u64, "PPTX retained metadata values")?;
                        metadata.insert(key.to_string(), serde_json::Value::String(value));
                    }
                }
            }
            Ok(Event::End(_)) => {
                budget.charge_item("PPTX metadata XML events")?;
                depth = depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_pptx: unmatched closing element in docProps/core.xml")
                })?;
                current = None;
            }
            Ok(Event::Eof) => break,
            Ok(_) => budget.charge_item("PPTX metadata XML events")?,
            Err(error) => anyhow::bail!("extract_pptx: invalid XML in docProps/core.xml: {error}"),
        }
        buffer.clear();
    }
    if !saw_element || depth != 0 {
        anyhow::bail!("extract_pptx: incomplete XML in docProps/core.xml");
    }
    Ok(metadata)
}

fn metadata_key(name: &[u8]) -> Option<&'static str> {
    match name {
        b"dc:title" => Some("title"),
        b"dc:creator" => Some("author"),
        b"dc:subject" => Some("subject"),
        b"dc:description" => Some("description"),
        b"cp:keywords" => Some("keywords"),
        b"cp:lastModifiedBy" => Some("last_modified_by"),
        b"dcterms:created" => Some("created"),
        b"dcterms:modified" => Some("modified"),
        b"cp:revision" => Some("revision"),
        b"cp:category" => Some("category"),
        _ => None,
    }
}
