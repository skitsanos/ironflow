use std::collections::HashMap;
use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::nodes::extract::resource::Budget;

#[derive(Default)]
pub(super) struct ContentTypes {
    defaults: HashMap<String, String>,
    overrides: HashMap<String, String>,
}

impl ContentTypes {
    pub(super) fn mime_type(&self, part_name: &str) -> String {
        if let Some(content_type) = self.overrides.get(&part_name.to_ascii_lowercase()) {
            return content_type.clone();
        }
        let extension = part_name
            .rsplit('/')
            .next()
            .and_then(|file_name| file_name.rsplit_once('.'))
            .map(|(_, extension)| extension.to_ascii_lowercase());
        extension
            .as_ref()
            .and_then(|extension| self.defaults.get(extension))
            .cloned()
            .unwrap_or_else(|| fallback_media_mime_type(part_name).to_string())
    }
}

pub(super) fn parse_content_types<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<ContentTypes> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut content_types = ContentTypes::default();
    let mut saw_element = false;
    let mut depth = 0_u64;
    let mut xml_version = XmlVersion::Implicit1_0;
    loop {
        budget.checkpoint()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Decl(declaration)) => {
                budget.charge_item("PPTX content-type XML events")?;
                xml_version = declaration
                    .xml_version()
                    .context("extract_pptx: invalid content-type XML declaration")?;
            }
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                budget.charge_item("PPTX content-type XML events")?;
                collect_definition(&event, xml_version, &mut content_types, budget)?;
            }
            Ok(Event::Empty(event)) => {
                saw_element = true;
                budget.charge_item("PPTX content-type XML events")?;
                collect_definition(&event, xml_version, &mut content_types, budget)?;
            }
            Ok(Event::End(_)) => {
                budget.charge_item("PPTX content-type XML events")?;
                depth = depth.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!(
                        "extract_pptx: unmatched closing element in [Content_Types].xml"
                    )
                })?;
            }
            Ok(Event::Eof) => break,
            Ok(_) => budget.charge_item("PPTX content-type XML events")?,
            Err(error) => {
                anyhow::bail!("extract_pptx: invalid XML in [Content_Types].xml: {error}")
            }
        }
        buffer.clear();
    }
    if !saw_element || depth != 0 {
        anyhow::bail!("extract_pptx: incomplete XML in [Content_Types].xml");
    }
    Ok(content_types)
}

fn collect_definition(
    event: &BytesStart<'_>,
    xml_version: XmlVersion,
    content_types: &mut ContentTypes,
    budget: &mut Budget<'_>,
) -> Result<()> {
    let event_name = event.name();
    let local_name = local_name(event_name.as_ref());
    if local_name != b"Default" && local_name != b"Override" {
        return Ok(());
    }
    budget.charge_item("PPTX content-type definitions")?;
    let mut attributes = decode_attributes(event, xml_version, local_name, budget)?;
    let content_type = required_attribute(&mut attributes, "ContentType", local_name)?;
    let key = if local_name == b"Default" {
        required_attribute(&mut attributes, "Extension", local_name)?.to_ascii_lowercase()
    } else {
        normalize_part_name(required_attribute(&mut attributes, "PartName", local_name)?)?
    };
    let definitions = if local_name == b"Default" {
        &mut content_types.defaults
    } else {
        &mut content_types.overrides
    };
    if definitions.insert(key.clone(), content_type).is_some() {
        anyhow::bail!("extract_pptx: duplicate content-type definition for '{key}'");
    }
    Ok(())
}

fn decode_attributes(
    event: &BytesStart<'_>,
    xml_version: XmlVersion,
    element: &[u8],
    budget: &mut Budget<'_>,
) -> Result<HashMap<String, String>> {
    let mut attributes = HashMap::new();
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid content-type attribute")?;
        let key = match (element, attribute.key.as_ref()) {
            (b"Default", b"Extension") => "Extension",
            (b"Override", b"PartName") => "PartName",
            (_, b"ContentType") => "ContentType",
            _ => continue,
        };
        let value = attribute
            .decoded_and_normalized_value(xml_version, event.decoder())
            .context("extract_pptx: invalid content-type attribute value")?;
        if key == "ContentType" {
            crate::artifacts::validate_mime_type(Some(value.as_ref())).map_err(|error| {
                anyhow::anyhow!("extract_pptx: invalid package ContentType: {error}")
            })?;
        }
        budget.charge_output(value.len() as u64, "PPTX retained content-type attributes")?;
        let value = value.into_owned();
        attributes.insert(key.to_string(), value);
    }
    Ok(attributes)
}

fn required_attribute(
    attributes: &mut HashMap<String, String>,
    name: &str,
    element: &[u8],
) -> Result<String> {
    attributes.remove(name).ok_or_else(|| {
        anyhow::anyhow!(
            "extract_pptx: {} content-type entry is missing {name}",
            String::from_utf8_lossy(element)
        )
    })
}

fn normalize_part_name(mut part_name: String) -> Result<String> {
    let normalized = part_name.strip_prefix('/').ok_or_else(|| {
        anyhow::anyhow!("extract_pptx: content-type PartName must start with '/': {part_name}")
    })?;
    if normalized.is_empty()
        || normalized.contains('\\')
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        anyhow::bail!("extract_pptx: invalid content-type PartName: {part_name}");
    }
    part_name.remove(0);
    part_name.make_ascii_lowercase();
    Ok(part_name)
}

fn fallback_media_mime_type(path: &str) -> &'static str {
    match path
        .rsplit('/')
        .next()
        .and_then(|file_name| file_name.rsplit_once('.'))
        .map(|(_, extension)| extension)
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::ContentTypes;

    #[test]
    fn mime_lookup_is_case_insensitive_and_ignores_dots_in_parent_directories() {
        let mut content_types = ContentTypes::default();
        content_types
            .defaults
            .insert("blob".into(), "image/jpeg".into());
        content_types
            .overrides
            .insert("ppt/media/image.blob".into(), "image/png".into());

        assert_eq!(content_types.mime_type("PPT/MEDIA/IMAGE.BLOB"), "image/png");
        assert_eq!(
            content_types.mime_type("ppt/media/other.BLOB"),
            "image/jpeg"
        );
        assert_eq!(
            content_types.mime_type("ppt/media.v1/image"),
            "application/octet-stream"
        );
    }
}
