use std::collections::HashMap;
use std::io::BufRead;

use anyhow::Result;

use super::super::resource::Budget;
use super::xml::{XmlDocument, visit_attributes};

/// Parse `word/theme/theme1.xml` into OOXML theme-name to hex-color mappings.
pub(in crate::nodes::extract) fn parse_theme_colors<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<HashMap<String, String>> {
    let mut colors = HashMap::new();
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().check_comments = true;
    let mut buf = Vec::new();
    let mut in_scheme = false;
    let mut current_role = None;
    let mut document = XmlDocument::new("word/theme/theme1.xml");

    loop {
        budget.charge_item("DOCX theme XML events")?;
        let event = reader.read_event_into(&mut buf).map_err(|error| {
            anyhow::anyhow!("extract_word: invalid word/theme/theme1.xml: {error}")
        })?;
        document.observe(&event, budget)?;
        match event {
            Event::Start(ref event) | Event::Empty(ref event) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "clrScheme" {
                    in_scheme = true;
                } else if in_scheme {
                    if let Some(role) = canonical_role(local) {
                        current_role = Some(role);
                    } else if (local == "srgbClr" || local == "sysClr")
                        && let Some(role) = current_role
                    {
                        let attr_name: &[u8] = if local == "srgbClr" {
                            b"val"
                        } else {
                            b"lastClr"
                        };
                        visit_attributes(
                            event,
                            "word/theme/theme1.xml",
                            budget,
                            |key, value, _| {
                                if key != attr_name {
                                    return Ok(());
                                }
                                let value = String::from_utf8_lossy(value).to_uppercase();
                                colors.insert(role.to_string(), value);
                                Ok(())
                            },
                        )?;
                    }
                }
            }
            Event::End(ref event) => {
                let raw = String::from_utf8_lossy(event.name().as_ref()).to_string();
                let local = raw.rsplit(':').next().unwrap_or(&raw);
                if local == "clrScheme" {
                    in_scheme = false;
                    current_role = None;
                } else if canonical_role(local).is_some() {
                    current_role = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(colors)
}

fn canonical_role(local: &str) -> Option<&'static str> {
    match local {
        "dk1" => Some("dark1"),
        "lt1" => Some("light1"),
        "dk2" => Some("dark2"),
        "lt2" => Some("light2"),
        "accent1" => Some("accent1"),
        "accent2" => Some("accent2"),
        "accent3" => Some("accent3"),
        "accent4" => Some("accent4"),
        "accent5" => Some("accent5"),
        "accent6" => Some("accent6"),
        "hlink" => Some("hyperlink"),
        "folHlink" => Some("followedHyperlink"),
        _ => None,
    }
}
