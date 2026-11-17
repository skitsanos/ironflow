//! Reads the parts outside the main story: the footnotes and endnotes the
//! body references, and, when requested, the header and footer parts its
//! sections name.
//!
//! Parts are found through `word/_rels/document.xml.rels` (Office for the
//! web names them `header.xml` and `footer.xml`, not `header1.xml`). Targets
//! are untrusted: internal only and inside the package. A reference whose
//! relationship, part or note is missing is skipped; its note marker stays
//! in the text without a definition.

use anyhow::Result;

use crate::nodes::extract::docx_parser::{
    DocxBlock, DocxFurniture, DocxNote, DocxReferences, DocxStories, FurnitureKind, Lookups,
    NoteKind, parse_docx_notes, parse_docx_story,
};
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::ooxml::part_paths::resolve_part;
use crate::nodes::extract::ooxml::relationships::{
    DOCX, Relationships, office_document_type, parse_relationships,
};
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

const OPERATION: &str = "extract_word";
const DOCUMENT: &str = "word/document.xml";
const DOCUMENT_RELATIONSHIPS: &str = "word/_rels/document.xml.rels";

#[derive(Clone, Copy)]
pub(super) struct StoryOptions {
    pub(super) notes: bool,
    pub(super) headers_footers: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Linked {
    Header,
    Footer,
    Footnotes,
    Endnotes,
    Other,
}

fn classify(value: &str) -> Linked {
    match office_document_type(value) {
        Some("header") => Linked::Header,
        Some("footer") => Linked::Footer,
        Some("footnotes") => Linked::Footnotes,
        Some("endnotes") => Linked::Endnotes,
        _ => Linked::Other,
    }
}

pub(super) struct Reader<'a, 'b, 'c> {
    pub(super) archive: &'a mut Archive,
    pub(super) lookups: &'a Lookups<'b>,
    pub(super) budget: &'a mut Budget<'c>,
    pub(super) execution: &'a ExecutionControl,
}

impl Reader<'_, '_, '_> {
    pub(super) fn read(
        &mut self,
        references: &DocxReferences,
        options: StoryOptions,
    ) -> Result<DocxStories> {
        let notes =
            options.notes && !(references.footnotes.is_empty() && references.endnotes.is_empty());
        let furniture = options.headers_footers && !references.furniture.is_empty();
        let mut stories = DocxStories::default();
        if !notes && !furniture {
            return Ok(stories);
        }
        let budget = &mut *self.budget;
        let relationships = self
            .archive
            .with_optional_xml(DOCUMENT_RELATIONSHIPS, self.execution, |reader| {
                parse_relationships(reader, &DOCX, classify, budget)
            })?
            .unwrap_or_default();
        if notes {
            let footnotes = &references.footnotes;
            stories.footnotes = self.notes(&relationships, NoteKind::Footnote, footnotes)?;
            let endnotes = &references.endnotes;
            stories.endnotes = self.notes(&relationships, NoteKind::Endnote, endnotes)?;
        }
        if furniture {
            self.furniture(&relationships, references, &mut stories)?;
        }
        Ok(stories)
    }

    fn notes(
        &mut self,
        relationships: &Relationships<Linked>,
        kind: NoteKind,
        ids: &[String],
    ) -> Result<Vec<DocxNote>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let (linked, fallback, label) = match kind {
            NoteKind::Footnote => (Linked::Footnotes, "word/footnotes.xml", "footnotes part"),
            NoteKind::Endnote => (Linked::Endnotes, "word/endnotes.xml", "endnotes part"),
        };
        let part = match single_target(relationships, linked, kind.name())? {
            Some(target) => self.resolve(target)?,
            None => fallback.to_string(),
        };
        let (lookups, budget) = (self.lookups, &mut *self.budget);
        let parsed = self
            .archive
            .with_optional_xml(&part, self.execution, |reader| {
                parse_docx_notes(reader, label, kind, ids, lookups, budget)
            })?
            .unwrap_or_default();
        let mut notes = Vec::new();
        for (index, (id, blocks)) in ids.iter().zip(parsed).enumerate() {
            self.budget.checkpoint()?;
            let Some(blocks) = blocks else {
                continue;
            };
            let label = kind.label(index + 1);
            self.budget
                .charge_output((label.len() + id.len()) as u64, "DOCX note labels")?;
            notes.push(DocxNote {
                label,
                id: id.clone(),
                blocks,
            });
        }
        Ok(notes)
    }

    fn furniture(
        &mut self,
        relationships: &Relationships<Linked>,
        references: &DocxReferences,
        stories: &mut DocxStories,
    ) -> Result<()> {
        let mut seen: Vec<String> = Vec::new();
        for reference in &references.furniture {
            self.budget.checkpoint()?;
            let Some(relationship) = relationships.get(&reference.relationship_id) else {
                continue;
            };
            let (expected, name, label) = match reference.kind {
                FurnitureKind::Header => (Linked::Header, "header", "header part"),
                FurnitureKind::Footer => (Linked::Footer, "footer", "footer part"),
            };
            anyhow::ensure!(
                relationship.kind == expected,
                "{OPERATION}: relationship '{}' is not a {name}",
                reference.relationship_id
            );
            anyhow::ensure!(
                !relationship.external,
                "{OPERATION}: external {name} relationship is not supported"
            );
            let part = self.resolve(&relationship.target)?;
            if seen.contains(&part) {
                continue;
            }
            seen.push(part.clone());
            let (lookups, budget) = (self.lookups, &mut *self.budget);
            let Some(blocks) = self
                .archive
                .with_optional_xml(&part, self.execution, |reader| {
                    parse_docx_story(reader, label, lookups, budget)
                })?
            else {
                continue;
            };
            if !has_text(&blocks) {
                continue;
            }
            self.budget
                .charge_output(reference.slot.len() as u64, "DOCX header and footer slots")?;
            let furniture = DocxFurniture {
                slot: reference.slot.clone(),
                part,
                blocks,
            };
            match reference.kind {
                FurnitureKind::Header => stories.headers.push(furniture),
                FurnitureKind::Footer => stories.footers.push(furniture),
            }
        }
        Ok(())
    }

    fn resolve(&mut self, target: &str) -> Result<String> {
        self.budget.admit_output(
            (DOCUMENT.len() + target.len()) as u64,
            "DOCX resolved part paths",
        )?;
        let part = resolve_part(OPERATION, DOCUMENT, target)?;
        self.budget
            .charge_output(part.len() as u64, "DOCX retained part paths")?;
        Ok(part)
    }
}

fn single_target<'r>(
    relationships: &'r Relationships<Linked>,
    linked: Linked,
    name: &str,
) -> Result<Option<&'r str>> {
    let mut found = None;
    for relationship in relationships.values() {
        if relationship.kind != linked {
            continue;
        }
        anyhow::ensure!(
            found.is_none(),
            "{OPERATION}: multiple {name}s relationships on {DOCUMENT}"
        );
        anyhow::ensure!(
            !relationship.external,
            "{OPERATION}: external {name}s relationship is not supported"
        );
        found = Some(relationship.target.as_str());
    }
    Ok(found)
}

fn has_text(blocks: &[DocxBlock]) -> bool {
    blocks.iter().any(|block| match block {
        DocxBlock::Paragraph(paragraph) => !paragraph.runs.is_empty(),
        DocxBlock::Table(table) => table.rows.iter().any(|row| {
            row.cells
                .iter()
                .any(|cell| cell.paragraphs.iter().any(|p| !p.runs.is_empty()))
        }),
    })
}
