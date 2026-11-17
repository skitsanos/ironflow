//! The parts of a WordprocessingML package, found through relationships.
//!
//! The main document is the target of the package `officeDocument`
//! relationship; its styles, numbering, theme, comments, notes, headers and
//! footers are targets of its own relationships part. The conventional
//! `word/...` names are the fallback when a package names no such part, as
//! hand-built and older packages often do. Targets are untrusted: internal
//! only, no scheme, no escape above the package root.

use anyhow::Result;

use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::ooxml::package::package_parts;
use crate::nodes::extract::ooxml::part_paths::{relationships_part, resolve_part};
use crate::nodes::extract::ooxml::relationships::{
    DOCX, Relationship, Relationships, office_document_type, parse_relationships,
};
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

const OPERATION: &str = "extract_word";
const MAIN_FALLBACK: &str = "word/document.xml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Linked {
    Styles,
    Numbering,
    Theme,
    Comments,
    Header,
    Footer,
    Footnotes,
    Endnotes,
    Other,
}

impl Linked {
    fn name(self) -> &'static str {
        match self {
            Linked::Styles => "styles",
            Linked::Numbering => "numbering",
            Linked::Theme => "theme",
            Linked::Comments => "comments",
            Linked::Header => "header",
            Linked::Footer => "footer",
            Linked::Footnotes => "footnotes",
            Linked::Endnotes => "endnotes",
            Linked::Other => "other",
        }
    }
}

fn classify(value: &str) -> Linked {
    match office_document_type(value) {
        Some("styles") => Linked::Styles,
        Some("numbering") => Linked::Numbering,
        Some("theme") => Linked::Theme,
        Some("comments") => Linked::Comments,
        Some("header") => Linked::Header,
        Some("footer") => Linked::Footer,
        Some("footnotes") => Linked::Footnotes,
        Some("endnotes") => Linked::Endnotes,
        _ => Linked::Other,
    }
}

pub(super) struct DocxPackage {
    pub(super) document: String,
    pub(super) core_properties: String,
    relationships: Relationships<Linked>,
}

impl DocxPackage {
    pub(super) fn open(
        archive: &mut Archive,
        budget: &mut Budget<'_>,
        execution: &ExecutionControl,
    ) -> Result<Self> {
        let parts = package_parts(archive, &DOCX, budget, execution)?;
        let document = parts.main.unwrap_or_else(|| MAIN_FALLBACK.to_string());
        // A resolved main part name was charged by `package_parts`; the
        // relationships part name is transient.
        budget.admit_output(document.len() as u64 + 12, "DOCX relationship part names")?;
        let relationships = archive
            .with_optional_xml(&relationships_part(&document), execution, |reader| {
                parse_relationships(reader, &DOCX, classify, budget)
            })?
            .unwrap_or_default();
        Ok(Self {
            document,
            core_properties: parts.core_properties,
            relationships,
        })
    }

    /// The single part of `linked` kind, or `fallback` when the document
    /// names none. Two such relationships, or an external one, fail.
    pub(super) fn part(
        &self,
        linked: Linked,
        fallback: &str,
        budget: &mut Budget<'_>,
    ) -> Result<String> {
        let mut found = None;
        for relationship in self.relationships.values() {
            budget.checkpoint()?;
            if relationship.kind != linked {
                continue;
            }
            let name = linked.name();
            anyhow::ensure!(
                found.is_none(),
                "{OPERATION}: multiple {name} relationships on {}",
                self.document
            );
            anyhow::ensure!(
                !relationship.external,
                "{OPERATION}: external {name} relationship is not supported"
            );
            found = Some(relationship.target.as_str());
        }
        match found {
            Some(target) => self.resolve(target, budget),
            None => Ok(fallback.to_string()),
        }
    }

    pub(super) fn relationship(&self, id: &str) -> Option<&Relationship<Linked>> {
        self.relationships.get(id)
    }

    /// Resolves a target of the main document's relationships.
    pub(super) fn resolve(&self, target: &str, budget: &mut Budget<'_>) -> Result<String> {
        budget.admit_output(
            (self.document.len() + target.len()) as u64,
            "DOCX resolved part paths",
        )?;
        let part = resolve_part(OPERATION, &self.document, target)?;
        budget.charge_output(part.len() as u64, "DOCX retained part paths")?;
        Ok(part)
    }
}
