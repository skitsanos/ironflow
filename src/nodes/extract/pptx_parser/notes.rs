use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, Event};

use crate::nodes::extract::resource::Budget;
use crate::util::xml::{Decoder, attribute_value};

/// Which text of the notes part is speaker notes. A notes slide holds the
/// notes in its body placeholder, next to slide-image, slide-number, date,
/// header and footer placeholders whose text is page furniture.
#[derive(Default)]
struct Scope {
    in_shape: bool,
    body: bool,
    in_field: bool,
}

impl Scope {
    /// Text counts inside the body placeholder, and in a part that has no
    /// shapes at all. Field runs (`a:fld`) are generated values, not notes.
    fn notes(&self) -> bool {
        !self.in_shape || self.body
    }

    fn open(
        &mut self,
        event: &BytesStart<'_>,
        version: quick_xml::XmlVersion,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        match local_name(event.name().as_ref().as_bytes()) {
            b"sp" => {
                self.in_shape = true;
                self.body = false;
            }
            b"ph" if self.in_shape => self.body = placeholder_type(event, version, budget)?,
            b"fld" => self.in_field = true,
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self, name: &[u8]) {
        match local_name(name) {
            b"sp" => {
                self.in_shape = false;
                self.body = false;
            }
            b"fld" => self.in_field = false,
            _ => {}
        }
    }
}

/// Whether a `p:ph` element declares the body placeholder.
fn placeholder_type(
    event: &BytesStart<'_>,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<bool> {
    let mut body = false;
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid speaker notes attribute")?;
        if attribute.key.as_ref().as_bytes() == b"type" {
            body = attribute_value(&attribute, version, |bytes| {
                budget.charge_output(bytes, "PPTX notes placeholder type")
            })? == "body";
        }
    }
    Ok(body)
}

pub(super) fn parse_pptx_notes<R: BufRead>(xml: R, budget: &mut Budget<'_>) -> Result<String> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut decoder = Decoder::default();
    let mut buffer = Vec::new();
    let mut text = String::new();
    let mut in_text = false;
    let mut scope = Scope::default();
    let mut saw_element = false;
    let mut depth = 0_u64;
    loop {
        budget.checkpoint()?;
        match reader
            .read_event_into(&mut buffer)
            .map_err(anyhow::Error::from)
            .and_then(|event| {
                decoder.decode(event, |bytes| {
                    budget.charge_output(bytes, "PPTX decoded notes text")
                })
            }) {
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                budget.charge_item("PPTX notes XML events")?;
                scope.open(&event, decoder.version, budget)?;
                in_text = local_name(event.name().as_ref().as_bytes()) == b"t"
                    && scope.notes()
                    && !scope.in_field;
            }
            Ok(Event::Empty(event)) => {
                saw_element = true;
                budget.charge_item("PPTX notes XML events")?;
                scope.open(&event, decoder.version, budget)?;
                scope.close(event.name().as_ref().as_bytes());
            }
            Ok(Event::Text(event)) if in_text => {
                budget.charge_item("PPTX notes XML events")?;
                text.push_str(event.as_ref());
            }
            Ok(Event::End(event)) => {
                let name = event.name();
                let name = name.as_ref().as_bytes();
                scope.close(name);
                if local_name(name) == b"p" && scope.notes() {
                    budget.charge_output(1, "PPTX notes paragraph separator")?;
                    text.push('\n');
                }
                budget.charge_item("PPTX notes XML events")?;
                depth = depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!("extract_pptx: unmatched closing element in speaker notes")
                })?;
                in_text = false;
            }
            Ok(Event::Eof) => break,
            Ok(_) => budget.charge_item("PPTX notes XML events")?,
            Err(error) => anyhow::bail!("extract_pptx: invalid XML in speaker notes: {error}"),
        }
        buffer.clear();
    }
    if !saw_element || depth != 0 {
        anyhow::bail!("extract_pptx: incomplete XML in speaker notes");
    }
    Ok(text.trim().to_string())
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}
