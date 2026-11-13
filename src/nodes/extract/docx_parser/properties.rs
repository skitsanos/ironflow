use anyhow::Result;
use quick_xml::events::BytesStart;

use super::super::resource::Budget;
use super::blocks::Lookups;
use super::{DocxParagraph, DocxRun, visit_attributes};

const PART: &str = "word/document.xml";

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
    version: quick_xml::XmlVersion,
    theme_colors: &std::collections::HashMap<String, String>,
    budget: &mut Budget<'_>,
) -> Result<()> {
    match name {
        "w:b" => run.bold = toggle(event, &["0", "false", "off"], version, budget)?,
        "w:i" => run.italic = toggle(event, &["0", "false", "off"], version, budget)?,
        "w:strike" => run.strikethrough = toggle(event, &["0", "false", "off"], version, budget)?,
        // `w:u` carries an underline style rather than an on/off value.
        "w:u" => run.underline = toggle(event, &["none"], version, budget)?,
        "w:color" => apply_run_color(event, run, theme_colors, version, budget)?,
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

/// A toggle property is on when present unless `w:val` names one of the
/// `off` values. The on/off type allows `1`, `true`, `on`, `0`, `false` and
/// `off`; an absent `w:val` means on.
fn toggle(
    event: &BytesStart<'_>,
    off: &[&str],
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<bool> {
    let mut on = true;
    visit_attributes(event, PART, version, budget, |key, value, _| {
        if key == b"w:val" {
            on = !off.iter().any(|candidate| candidate.as_bytes() == value);
        }
        Ok(())
    })?;
    Ok(on)
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
