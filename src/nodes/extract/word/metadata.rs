use std::collections::BTreeMap;
use std::io::BufRead;

use anyhow::Result;

use super::super::docx_parser::XmlDocument;
use super::super::ooxml::core_properties::property_key;
use super::super::resource::Budget;

pub(super) fn extract_docx_metadata<R: BufRead>(
    xml: R,
    part: &str,
    budget: &mut Budget<'_>,
) -> Result<BTreeMap<String, String>> {
    use quick_xml::events::Event;

    let mut metadata = BTreeMap::<String, String>::new();
    let mut reader = quick_xml::NsReader::from_reader(xml);
    reader.config_mut().check_comments = true;
    let mut buf = Vec::new();
    let mut current = None;
    let mut document = XmlDocument::new("core properties part");
    loop {
        budget.charge_item("DOCX metadata XML events")?;
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|error| anyhow::anyhow!("extract_word: invalid {part}: {error}"))?;
        let event = document.decode(event, budget)?;
        match event {
            Event::Start(ref event) => {
                let (namespace, local) = reader.resolver().resolve_element(event.name());
                current = property_key(namespace, local.as_ref());
            }
            Event::Text(ref event) => {
                if let Some(key) = current {
                    metadata
                        .entry(key.to_string())
                        .or_default()
                        .push_str(event.as_ref());
                }
            }
            Event::End(_) => current = None,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    metadata.retain(|_, value| {
        *value = value.trim().to_owned();
        !value.is_empty()
    });
    Ok(metadata)
}
