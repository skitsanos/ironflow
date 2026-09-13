use std::borrow::Cow;

use anyhow::{Context, Result};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesText, Event, attributes::Attribute};

/// Converts character-bearing events to normalized Text, never recursively
/// expanding entities. Callers must not unescape the returned text again.
pub(crate) struct Decoder {
    pub(crate) version: XmlVersion,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            version: XmlVersion::Implicit1_0,
        }
    }
}

impl Decoder {
    pub(crate) fn decode<'a>(
        &mut self,
        event: Event<'a>,
        mut charge: impl FnMut(u64) -> Result<()>,
    ) -> Result<Event<'a>> {
        let text = match event {
            Event::Decl(ref declaration) => {
                self.version = declaration
                    .xml_version()
                    .context("invalid XML declaration")?;
                return Ok(event);
            }
            Event::DocType(_) => anyhow::bail!("XML DTDs are not supported"),
            Event::Text(text) => {
                // Normalization cannot grow text. Admit its source width before
                // quick-xml may allocate an EOL-normalized copy.
                charge(text.len() as u64)?;
                text.xml_content(self.version)
            }
            Event::CData(text) => {
                charge(text.len() as u64)?;
                text.xml_content(self.version)
            }
            Event::GeneralRef(reference) => {
                if let Some(character) = reference
                    .resolve_char_ref()
                    .context("invalid XML character reference")?
                {
                    validate_char(character, self.version)?;
                    charge(character.len_utf8() as u64)?;
                    Cow::Owned(character.to_string())
                } else {
                    let text = quick_xml::escape::resolve_xml_entity(reference.as_ref())
                        .ok_or_else(|| anyhow::anyhow!("unknown XML entity reference"))?;
                    charge(text.len() as u64)?;
                    Cow::Borrowed(text)
                }
            }
            _ => return Ok(event),
        };
        Ok(Event::Text(BytesText::from_escaped(text)))
    }
}

pub(crate) fn attribute_value<'a>(
    attribute: &Attribute<'a>,
    version: XmlVersion,
    mut charge: impl FnMut(u64) -> Result<()>,
) -> Result<Cow<'a, str>> {
    // Only XML's five predefined entities and numeric references are allowed;
    // their normalized value cannot exceed its encoded UTF-8 source width.
    charge(attribute.value.len() as u64)?;
    let value = attribute
        .normalized_value_with(version, 1, quick_xml::escape::resolve_xml_entity)
        .context("invalid XML attribute value")?;
    for character in value.chars() {
        validate_char(character, version)?;
    }
    Ok(value)
}

fn validate_char(character: char, version: XmlVersion) -> Result<()> {
    let valid = match version {
        XmlVersion::Explicit1_1 => {
            matches!(character, '\u{1}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
        }
        _ => {
            matches!(character, '\t' | '\n' | '\r' | ' '..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
        }
    };
    anyhow::ensure!(
        valid,
        "invalid XML character reference or attribute character"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
