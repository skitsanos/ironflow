//! JSON for note references, note definitions, headers and footers.

use anyhow::Result;
use serde_json::{Map, Value};

use super::super::super::docx_parser::{DocxFurniture, DocxNote, DocxStories, NoteRef};
use super::super::super::resource::Budget;
use super::{blocks_array, insert_string, insert_value};

/// `"note": {"type": "footnote", "label": "1"}` on a referencing run.
pub(super) fn insert_note_ref(
    object: &mut Map<String, Value>,
    note: &NoteRef,
    budget: &mut Budget<'_>,
) -> Result<()> {
    let mut value = Map::new();
    insert_string(
        &mut value,
        "type",
        note.kind.name(),
        "DOCX JSON note types",
        budget,
    )?;
    insert_string(
        &mut value,
        "label",
        &note.label,
        "DOCX JSON note labels",
        budget,
    )?;
    if let Some(mark) = &note.mark {
        insert_string(&mut value, "mark", mark, "DOCX JSON note marks", budget)?;
    }
    insert_value(object, "note", Value::Object(value), budget)
}

pub(super) fn insert_stories(
    object: &mut Map<String, Value>,
    stories: &DocxStories,
    budget: &mut Budget<'_>,
) -> Result<()> {
    insert_notes(object, "footnotes", &stories.footnotes, budget)?;
    insert_notes(object, "endnotes", &stories.endnotes, budget)?;
    insert_furniture(object, "headers", &stories.headers, budget)?;
    insert_furniture(object, "footers", &stories.footers, budget)
}

fn insert_notes(
    object: &mut Map<String, Value>,
    key: &str,
    notes: &[DocxNote],
    budget: &mut Budget<'_>,
) -> Result<()> {
    if notes.is_empty() {
        return Ok(());
    }
    let mut values = Vec::new();
    for note in notes {
        budget.checkpoint()?;
        let mut value = Map::new();
        insert_string(
            &mut value,
            "label",
            &note.label,
            "DOCX JSON note labels",
            budget,
        )?;
        insert_string(&mut value, "id", &note.id, "DOCX JSON note ids", budget)?;
        if let Some(mark) = &note.mark {
            insert_string(&mut value, "mark", mark, "DOCX JSON note marks", budget)?;
        }
        let blocks = blocks_array(&note.blocks, budget)?;
        insert_value(&mut value, "blocks", Value::Array(blocks), budget)?;
        values.push(Value::Object(value));
    }
    insert_value(object, key, Value::Array(values), budget)
}

fn insert_furniture(
    object: &mut Map<String, Value>,
    key: &str,
    parts: &[DocxFurniture],
    budget: &mut Budget<'_>,
) -> Result<()> {
    if parts.is_empty() {
        return Ok(());
    }
    let mut values = Vec::new();
    for furniture in parts {
        budget.checkpoint()?;
        let mut value = Map::new();
        insert_string(
            &mut value,
            "type",
            &furniture.slot,
            "DOCX JSON header and footer types",
            budget,
        )?;
        insert_string(
            &mut value,
            "part",
            &furniture.part,
            "DOCX JSON header and footer parts",
            budget,
        )?;
        let blocks = blocks_array(&furniture.blocks, budget)?;
        insert_value(&mut value, "blocks", Value::Array(blocks), budget)?;
        values.push(Value::Object(value));
    }
    insert_value(object, key, Value::Array(values), budget)
}
