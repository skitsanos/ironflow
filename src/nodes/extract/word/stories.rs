//! Reads the parts outside the main story: the footnotes and endnotes the
//! body references, and, when requested, the header and footer parts its
//! sections name.
//!
//! Parts are found through the main document's relationships (Office for
//! the web names them `header.xml` and `footer.xml`, not `header1.xml`); see
//! `package`. A reference whose relationship, part or note is missing is
//! skipped; its note marker stays in the text without a definition.

use anyhow::Result;

use super::package::{DocxPackage, Linked};
use crate::nodes::extract::docx_parser::{
    DocxBlock, DocxFurniture, DocxNote, DocxReferences, DocxStories, FurnitureKind, Lookups,
    NoteKind, parse_docx_notes, parse_docx_story,
};
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

const OPERATION: &str = "extract_word";

#[derive(Clone, Copy)]
pub(super) struct StoryOptions {
    pub(super) notes: bool,
    pub(super) headers_footers: bool,
}

pub(super) struct Reader<'a, 'b, 'c> {
    pub(super) archive: &'a mut Archive,
    pub(super) package: &'a DocxPackage,
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
        if notes {
            stories.footnotes =
                self.notes(NoteKind::Footnote, &references.footnotes, references)?;
            stories.endnotes = self.notes(NoteKind::Endnote, &references.endnotes, references)?;
        }
        if furniture {
            self.furniture(references, &mut stories)?;
        }
        Ok(stories)
    }

    fn notes(
        &mut self,
        kind: NoteKind,
        ids: &[String],
        references: &DocxReferences,
    ) -> Result<Vec<DocxNote>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let (linked, fallback, label) = match kind {
            NoteKind::Footnote => (Linked::Footnotes, "word/footnotes.xml", "footnotes part"),
            NoteKind::Endnote => (Linked::Endnotes, "word/endnotes.xml", "endnotes part"),
        };
        let part = self.package.part(linked, fallback, self.budget)?;
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
            let mark = references.marks.get(&label).cloned();
            notes.push(DocxNote {
                mark,
                label,
                id: id.clone(),
                blocks,
            });
        }
        Ok(notes)
    }

    fn furniture(&mut self, references: &DocxReferences, stories: &mut DocxStories) -> Result<()> {
        let mut seen: Vec<String> = Vec::new();
        for reference in &references.furniture {
            self.budget.checkpoint()?;
            let Some(relationship) = self.package.relationship(&reference.relationship_id) else {
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
            let part = self.package.resolve(&relationship.target, self.budget)?;
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
