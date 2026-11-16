//! Attribute readers for slide elements.

use anyhow::{Context, Result};
use quick_xml::events::BytesStart;

use super::State;
use crate::nodes::extract::resource::Budget;
use crate::util::xml::attribute_value;

pub(super) fn collect_string_attribute(
    event: &BytesStart<'_>,
    key: &[u8],
    target: &mut Option<String>,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<()> {
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid slide attribute")?;
        if attribute.key.as_ref().as_bytes() == key {
            *target = Some(
                attribute_value(&attribute, version, |bytes| {
                    budget.charge_output(bytes, "PPTX retained slide attributes")
                })?
                .into_owned(),
            );
        }
    }
    Ok(())
}

pub(super) fn collect_embed_id(
    event: &BytesStart<'_>,
    state: &mut State,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<()> {
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid image attribute")?;
        if local_name(attribute.key.as_ref().as_bytes()) == b"embed" {
            state.picture_embed_id = Some(
                attribute_value(&attribute, version, |bytes| {
                    budget.charge_output(bytes, "PPTX retained image relationship IDs")
                })?
                .into_owned(),
            );
        }
    }
    Ok(())
}

pub(super) fn collect_list_level(
    event: &BytesStart<'_>,
    state: &mut State,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<()> {
    for attribute in event.attributes() {
        let attribute = attribute.context("extract_pptx: invalid paragraph attribute")?;
        if attribute.key.as_ref().as_bytes() == b"lvl" {
            state.bullets.level(
                attribute_value(&attribute, version, |bytes| {
                    budget.charge_output(bytes, "PPTX list level decoding")
                })?
                .parse::<u32>()
                .context("extract_pptx: paragraph list level must be an unsigned integer")?,
            );
        }
    }
    Ok(())
}

pub(super) fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}
