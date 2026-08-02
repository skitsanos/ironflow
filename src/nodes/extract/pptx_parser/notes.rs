use std::io::BufRead;

use anyhow::Result;
use quick_xml::events::Event;

use crate::nodes::extract::resource::Budget;

pub(super) fn parse_pptx_notes<R: BufRead>(xml: R, budget: &mut Budget<'_>) -> Result<String> {
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut text = String::new();
    let mut in_text = false;
    let mut saw_element = false;
    let mut depth = 0_u64;
    loop {
        budget.checkpoint()?;
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                saw_element = true;
                depth = depth.saturating_add(1);
                budget.charge_item("PPTX notes XML events")?;
                in_text = local_name(event.name().as_ref()) == b"t";
            }
            Ok(Event::Empty(_)) => {
                saw_element = true;
                budget.charge_item("PPTX notes XML events")?;
            }
            Ok(Event::Text(event)) if in_text => {
                budget.charge_item("PPTX notes XML events")?;
                budget.charge_output(event.len() as u64 + 1, "PPTX retained notes")?;
                text.push_str(&String::from_utf8_lossy(event.as_ref()));
                text.push('\n');
            }
            Ok(Event::End(_)) => {
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
