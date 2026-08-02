use std::collections::{HashMap, HashSet};

use anyhow::Result;
use quick_xml::events::{BytesStart, Event};

use super::super::resource::Budget;
use super::visit_attributes;

pub(super) struct AnchorCollector<'set, 'id> {
    comment_ids: &'set HashSet<&'id str>,
    open: HashSet<String>,
    anchors: HashMap<String, String>,
    in_text: bool,
}

impl<'set, 'id> AnchorCollector<'set, 'id> {
    pub(super) fn new(comment_ids: &'set HashSet<&'id str>) -> Self {
        Self {
            comment_ids,
            open: HashSet::new(),
            anchors: HashMap::new(),
            in_text: false,
        }
    }

    pub(super) fn observe(&mut self, event: &Event<'_>, budget: &mut Budget<'_>) -> Result<()> {
        budget.charge_item("DOCX document comment-range events")?;
        let is_empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(event) | Event::Empty(event) => match event.name().as_ref() {
                b"w:commentRangeStart" => self.update_ranges(event, true, budget)?,
                b"w:commentRangeEnd" => self.update_ranges(event, false, budget)?,
                b"w:t" => self.in_text = !is_empty,
                _ => {}
            },
            Event::Text(event) if self.in_text && !self.open.is_empty() => {
                self.append_text(event.as_ref(), budget)?;
            }
            Event::End(event) if event.name().as_ref() == b"w:t" => self.in_text = false,
            Event::Eof if !self.open.is_empty() => {
                anyhow::bail!(
                    "extract_word: {} unclosed comment range(s) in word/document.xml",
                    self.open.len()
                );
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn finish(self) -> HashMap<String, String> {
        self.anchors
    }

    fn append_text(&mut self, raw: &[u8], budget: &mut Budget<'_>) -> Result<()> {
        let text = String::from_utf8_lossy(raw);
        let open_count = u64::try_from(self.open.len()).unwrap_or(u64::MAX);
        budget.charge_items(open_count, "DOCX comment anchor fan-out")?;
        let matched = self
            .open
            .iter()
            .filter(|id| self.comment_ids.contains(id.as_str()))
            .count() as u64;
        let separators = self
            .open
            .iter()
            .filter(|id| {
                self.comment_ids.contains(id.as_str())
                    && self
                        .anchors
                        .get(id.as_str())
                        .is_some_and(|text| !text.is_empty())
            })
            .count() as u64;
        budget.charge_output(
            (text.len() as u64)
                .saturating_mul(matched)
                .saturating_add(separators),
            "DOCX anchored comment text",
        )?;
        for id in self
            .open
            .iter()
            .filter(|id| self.comment_ids.contains(id.as_str()))
        {
            let anchor = self.anchors.entry(id.clone()).or_default();
            if !anchor.is_empty() {
                anchor.push(' ');
            }
            anchor.push_str(&text);
        }
        Ok(())
    }

    fn update_ranges(
        &mut self,
        event: &BytesStart<'_>,
        insert: bool,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        visit_attributes(event, "word/document.xml", budget, |key, value, budget| {
            if key != b"w:id" {
                return Ok(());
            }
            if insert {
                budget.charge_item("DOCX open comment ranges")?;
            }
            let id = String::from_utf8_lossy(value).to_string();
            if insert {
                self.open.insert(id);
            } else if !self.open.remove(&id) {
                anyhow::bail!("extract_word: comment range {id} ended before it was opened");
            }
            Ok(())
        })
    }
}
