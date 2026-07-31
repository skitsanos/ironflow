use std::collections::HashMap;
use std::io::BufRead;

use anyhow::Result;

use super::super::resource::Budget;
use super::xml::{XmlDocument, attribute_value};

/// Parse `word/numbering.xml` into `numId -> is_numbered` mappings.
pub(in crate::nodes::extract) fn parse_numbering_defs<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<HashMap<String, bool>> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().check_comments = true;
    let mut buf = Vec::new();
    let mut abstract_defs = HashMap::<String, bool>::new();
    let mut num_to_abstract = HashMap::<String, String>::new();
    let mut current_abstract_id = None;
    let mut current_num_id = None;
    let mut document = XmlDocument::new("word/numbering.xml");

    loop {
        budget.charge_item("DOCX numbering XML events")?;
        let event = reader.read_event_into(&mut buf).map_err(|error| {
            anyhow::anyhow!("extract_word: invalid word/numbering.xml: {error}")
        })?;
        document.observe(&event, budget)?;
        match event {
            Event::Start(ref event) | Event::Empty(ref event) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                match name.as_str() {
                    "w:abstractNum" => {
                        current_abstract_id = attribute_value(
                            event,
                            b"w:abstractNumId",
                            "word/numbering.xml",
                            budget,
                        )?;
                    }
                    "w:numFmt" => {
                        if let (Some(id), Some(value)) = (
                            &current_abstract_id,
                            attribute_value(event, b"w:val", "word/numbering.xml", budget)?,
                        ) {
                            abstract_defs.insert(id.clone(), value != "bullet" && value != "none");
                        }
                    }
                    "w:num" => {
                        current_num_id =
                            attribute_value(event, b"w:numId", "word/numbering.xml", budget)?;
                    }
                    "w:abstractNumId" => {
                        if let (Some(num_id), Some(abstract_id)) = (
                            &current_num_id,
                            attribute_value(event, b"w:val", "word/numbering.xml", budget)?,
                        ) {
                            num_to_abstract.insert(num_id.clone(), abstract_id);
                        }
                    }
                    _ => {}
                }
            }
            Event::End(ref event) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if name == "w:abstractNum" {
                    current_abstract_id = None;
                } else if name == "w:num" {
                    current_num_id = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(num_to_abstract
        .into_iter()
        .filter_map(|(num_id, abstract_id)| {
            abstract_defs
                .get(&abstract_id)
                .copied()
                .map(|is_numbered| (num_id, is_numbered))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::parse_numbering_defs;

    #[tokio::test]
    async fn resolves_ordered_and_bulleted_numbering_instances() {
        let xml = r#"<w:numbering xmlns:w="urn:test">
                    <w:abstractNum w:abstractNumId="10">
                        <w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl>
                    </w:abstractNum>
                    <w:abstractNum w:abstractNumId="11">
                        <w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl>
                    </w:abstractNum>
                    <w:num w:numId="42"><w:abstractNumId w:val="10"/></w:num>
                    <w:num w:numId="43"><w:abstractNumId w:val="11"/></w:num>
                </w:numbering>"#;
        let numbering = crate::util::execution::run_blocking_step(move |execution| {
            let limits = crate::nodes::extract::resource::Limits::current();
            let mut budget =
                crate::nodes::extract::resource::Budget::new("extract_word", limits, &execution);
            parse_numbering_defs(xml.as_bytes(), &mut budget)
        })
        .await
        .unwrap();

        assert_eq!(numbering.get("42"), Some(&true));
        assert_eq!(numbering.get("43"), Some(&false));
    }
}
