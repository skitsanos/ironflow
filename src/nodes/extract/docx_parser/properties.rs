use anyhow::Result;

use super::super::resource::Budget;
use super::{DocxRun, visit_attributes};

pub(super) fn apply_run_color(
    event: &quick_xml::events::BytesStart<'_>,
    run: &mut DocxRun,
    theme_colors: &std::collections::HashMap<String, String>,
    version: quick_xml::XmlVersion,
    budget: &mut Budget<'_>,
) -> Result<()> {
    let mut hex = None;
    let mut theme = None;
    visit_attributes(
        event,
        "word/document.xml",
        version,
        budget,
        |key, value, _| {
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
        },
    )?;
    run.color = hex.or_else(|| theme.and_then(|key| theme_colors.get(&key).cloned()));
    Ok(())
}
