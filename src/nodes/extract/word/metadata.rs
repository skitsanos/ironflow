use std::collections::BTreeMap;
use std::io::BufRead;

use anyhow::Result;

use super::super::docx_parser::XmlDocument;
use super::super::resource::Budget;

const KNOWN_TAGS: [&str; 10] = [
    "dc:title",
    "dc:creator",
    "dc:subject",
    "dc:description",
    "cp:keywords",
    "cp:lastModifiedBy",
    "dcterms:created",
    "dcterms:modified",
    "cp:revision",
    "cp:category",
];

pub(super) fn extract_docx_metadata<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<BTreeMap<String, String>> {
    parse_core_properties(xml, budget)
}

fn parse_core_properties<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<BTreeMap<String, String>> {
    use quick_xml::events::Event;

    let mut metadata = BTreeMap::new();
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().check_comments = true;
    let mut buf = Vec::new();
    let mut current_tag = String::new();
    let mut in_metadata = false;
    let mut document = XmlDocument::new("docProps/core.xml");
    loop {
        budget.charge_item("DOCX metadata XML events")?;
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|error| anyhow::anyhow!("extract_word: invalid docProps/core.xml: {error}"))?;
        document.observe(&event, budget)?;
        match event {
            Event::Start(ref event) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if KNOWN_TAGS.contains(&name.as_str()) {
                    current_tag = name;
                    in_metadata = true;
                }
            }
            Event::Text(ref event) if in_metadata => {
                let text = String::from_utf8_lossy(event.as_ref()).trim().to_string();
                if !text.is_empty() {
                    budget.charge_output(text.len() as u64, "DOCX metadata value")?;
                    metadata.insert(key_for_tag(&current_tag).to_string(), text);
                }
            }
            Event::End(_) => in_metadata = false,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(metadata)
}

fn key_for_tag(tag: &str) -> &'static str {
    match tag {
        "dc:title" => "title",
        "dc:creator" => "author",
        "dc:subject" => "subject",
        "dc:description" => "description",
        "cp:keywords" => "keywords",
        "cp:lastModifiedBy" => "last_modified_by",
        "dcterms:created" => "created",
        "dcterms:modified" => "modified",
        "cp:revision" => "revision",
        "cp:category" => "category",
        _ => "unknown",
    }
}
