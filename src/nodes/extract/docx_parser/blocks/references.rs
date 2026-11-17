//! Footnote, endnote, header and footer references met in the body.
//!
//! A note reference gets the next label of its kind in reference order and
//! writes the `[^label]` marker into its run, so every format marks the
//! position. Header and footer references are only recorded here; the parts
//! they name are read afterwards, and only when requested.

use std::collections::HashMap;

use anyhow::Result;
use quick_xml::events::BytesStart;

use super::super::super::resource::Budget;
use super::super::xml::attribute_value;
use super::super::{DocxReferences, FurnitureKind, FurnitureRef, NoteKind, NoteRef};
use super::BlockBuilder;

const PART: &str = "word/document.xml";

#[derive(Default)]
pub(super) struct References {
    found: DocxReferences,
    footnote_labels: HashMap<String, usize>,
    endnote_labels: HashMap<String, usize>,
    /// Open `w:del` elements: a deleted reference is not part of the text.
    deleted: u64,
    /// False when the caller asked for no notes: references mark nothing.
    pub(super) mark_notes: bool,
}

impl References {
    pub(super) fn finish(self) -> DocxReferences {
        self.found
    }

    pub(super) fn leave_deletion(&mut self) {
        self.deleted = self.deleted.saturating_sub(1);
    }

    fn label(&mut self, kind: NoteKind, id: String, budget: &mut Budget<'_>) -> Result<String> {
        let (labels, ids) = match kind {
            NoteKind::Footnote => (&mut self.footnote_labels, &mut self.found.footnotes),
            NoteKind::Endnote => (&mut self.endnote_labels, &mut self.found.endnotes),
        };
        let ordinal = match labels.get(&id) {
            Some(ordinal) => *ordinal,
            None => {
                budget.charge_item("DOCX note references")?;
                // The id is kept twice: in the lookup and in label order.
                budget.charge_output(2 * id.len() as u64, "DOCX note reference ids")?;
                ids.push(id.clone());
                labels.insert(id, ids.len());
                ids.len()
            }
        };
        Ok(kind.label(ordinal))
    }
}

impl BlockBuilder {
    /// Handle a reference element, or a `w:del` that hides the ones inside.
    pub(super) fn open_reference(
        &mut self,
        name: &str,
        event: &BytesStart<'_>,
        is_empty: bool,
        version: quick_xml::XmlVersion,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        if name == "w:del" {
            self.references.deleted += u64::from(!is_empty);
            return Ok(());
        }
        if self.references.deleted > 0 {
            return Ok(());
        }
        match name {
            "w:footnoteReference" => {
                self.note_reference(NoteKind::Footnote, event, version, budget)
            }
            "w:endnoteReference" => self.note_reference(NoteKind::Endnote, event, version, budget),
            "w:headerReference" => {
                self.furniture_reference(FurnitureKind::Header, event, version, budget)
            }
            "w:footerReference" => {
                self.furniture_reference(FurnitureKind::Footer, event, version, budget)
            }
            _ => Ok(()),
        }
    }

    fn note_reference(
        &mut self,
        kind: NoteKind,
        event: &BytesStart<'_>,
        version: quick_xml::XmlVersion,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        // A reference outside a run, or without an id, marks nothing.
        if !self.references.mark_notes || self.frames.last().is_none_or(|frame| frame.run.is_none())
        {
            return Ok(());
        }
        let Some(id) = attribute_value(event, b"w:id", PART, version, budget)? else {
            return Ok(());
        };
        let label = self.references.label(kind, id, budget)?;
        if let Some(run) = self.frames.last_mut().and_then(|frame| frame.run.as_mut()) {
            budget.charge_output(label.len() as u64 + 3, "DOCX note reference markers")?;
            run.text.push_str("[^");
            run.text.push_str(&label);
            run.text.push(']');
            run.note = Some(NoteRef { kind, label });
        }
        Ok(())
    }

    fn furniture_reference(
        &mut self,
        kind: FurnitureKind,
        event: &BytesStart<'_>,
        version: quick_xml::XmlVersion,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        let mut slot = None;
        let mut relationship_id = None;
        super::super::visit_attributes(event, PART, version, budget, |key, value, _| {
            match key {
                b"w:type" => slot = Some(String::from_utf8_lossy(value).into_owned()),
                b"r:id" => relationship_id = Some(String::from_utf8_lossy(value).into_owned()),
                _ => {}
            }
            Ok(())
        })?;
        let Some(relationship_id) = relationship_id else {
            return Ok(());
        };
        let furniture = &mut self.references.found.furniture;
        if furniture
            .iter()
            .any(|known| known.kind == kind && known.relationship_id == relationship_id)
        {
            return Ok(());
        }
        let slot = slot.unwrap_or_else(|| "default".to_string());
        budget.charge_item("DOCX header and footer references")?;
        budget.charge_output(
            (slot.len() + relationship_id.len()) as u64,
            "DOCX header and footer reference ids",
        )?;
        furniture.push(FurnitureRef {
            kind,
            slot,
            relationship_id,
        });
        Ok(())
    }
}
