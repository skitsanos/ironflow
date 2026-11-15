use anyhow::Result;
use quick_xml::events::BytesStart;

use super::super::resource::Budget;
use super::blocks::Lookups;
use super::styles::{Direct, StyleSheet, on_off, toggle_index};
use super::xml::attribute_value;
use super::{DocxParagraph, DocxRun, visit_attributes};

const PART: &str = "word/document.xml";

/// Formatting set directly on a run that is resolved against the styles
/// when the run closes.
#[derive(Default)]
pub(super) struct RunFormat {
    toggles: Direct,
    style: Option<String>,
}

impl RunFormat {
    /// Set the bold, italic and strikethrough flags of `run` from its direct
    /// toggles, its character style and the style of its paragraph.
    pub(super) fn resolve(
        &self,
        run: &mut DocxRun,
        paragraph: &DocxParagraph,
        styles: &StyleSheet,
    ) {
        [run.bold, run.italic, run.strikethrough] = styles.resolve(
            paragraph.style.as_deref(),
            self.style.as_deref(),
            self.toggles,
        );
    }
}

/// Apply one child of `w:pPr` to the paragraph being built.
pub(super) fn apply_paragraph_property(
    name: &str,
    event: &BytesStart<'_>,
    paragraph: &mut DocxParagraph,
    version: quick_xml::XmlVersion,
    lookups: &Lookups<'_>,
    budget: &mut Budget<'_>,
) -> Result<()> {
    match name {
        "w:pStyle" => {
            visit_attributes(event, PART, version, budget, |key, value, _| {
                if key == b"w:val" {
                    paragraph.style = Some(String::from_utf8_lossy(value).to_string());
                }
                Ok(())
            })?;
            let style = paragraph.style.as_deref();
            paragraph.style_emphasis = lookups.styles.resolve(style, None, Direct::default());
        }
        "w:numPr" => paragraph.is_list_item = true,
        "w:ilvl" => {
            visit_attributes(event, PART, version, budget, |key, value, budget| {
                if key == b"w:val"
                    && let Ok(level) = String::from_utf8_lossy(value).parse::<u32>()
                {
                    budget.charge_items(u64::from(level), "DOCX list indentation work")?;
                    paragraph.list_level = level;
                }
                Ok(())
            })?;
        }
        "w:numId" => {
            visit_attributes(event, PART, version, budget, |key, value, _| {
                if key == b"w:val" {
                    paragraph.is_numbered = lookups
                        .numbering_defs
                        .get(String::from_utf8_lossy(value).as_ref())
                        .copied()
                        .unwrap_or(false);
                }
                Ok(())
            })?;
        }
        _ => {}
    }
    Ok(())
}

/// Apply one child of `w:rPr` to the run being built.
pub(super) fn apply_run_property(
    name: &str,
    event: &BytesStart<'_>,
    run: &mut DocxRun,
    format: &mut RunFormat,
    version: quick_xml::XmlVersion,
    lookups: &Lookups<'_>,
    budget: &mut Budget<'_>,
) -> Result<()> {
    if let Some(index) = toggle_index(name) {
        format.toggles[index] = Some(on_off(event, PART, version, budget)?);
        return Ok(());
    }
    match name {
        "w:rStyle" => {
            format.style = attribute_value(event, b"w:val", PART, version, budget)?;
            let length = format.style.as_ref().map_or(0, String::len);
            budget.charge_output(length as u64, "DOCX retained style references")?;
        }
        // `w:u` carries an underline style rather than an on/off value: any
        // style but `none` underlines, and an absent `w:val` does too.
        "w:u" => {
            let style = attribute_value(event, b"w:val", PART, version, budget)?;
            run.underline = style.as_deref() != Some("none");
        }
        "w:color" => apply_run_color(event, run, lookups.theme_colors, version, budget)?,
        "w:highlight" => {
            visit_attributes(event, PART, version, budget, |key, value, _| {
                if key == b"w:val" {
                    let value = String::from_utf8_lossy(value).to_string();
                    if value != "none" && !value.is_empty() {
                        run.highlight = Some(value);
                    }
                }
                Ok(())
            })?;
        }
        _ => {}
    }
    Ok(())
}

fn apply_run_color(
    event: &BytesStart<'_>,
    run: &mut DocxRun,
    theme_colors: &std::collections::HashMap<String, String>,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<()> {
    let mut hex = None;
    let mut theme = None;
    visit_attributes(event, PART, version, budget, |key, value, _| {
        match key {
            b"w:val" => {
                let value = String::from_utf8_lossy(value).to_string();
                if value != "auto" && !value.is_empty() {
                    hex = Some(value.to_uppercase());
                }
            }
            b"w:themeColor" => theme = Some(String::from_utf8_lossy(value).to_string()),
            _ => {}
        }
        Ok(())
    })?;
    run.color = hex.or_else(|| theme.and_then(|key| theme_colors.get(&key).cloned()));
    Ok(())
}
