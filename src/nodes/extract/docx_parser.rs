mod anchors;
mod blocks;
mod numbering;
mod properties;
mod stories;
mod styles;
mod theme;
mod xml;

use std::io::BufRead;

use anyhow::Result;

use anchors::AnchorCollector;
use blocks::BlockBuilder;
pub(super) use blocks::Lookups;
pub(super) use numbering::parse_numbering_defs;
pub(super) use stories::{parse_docx_notes, parse_docx_story};
pub(super) use styles::{Flags, StyleSheet, parse_styles};
pub(super) use theme::parse_theme_colors;
pub(super) use xml::{XmlDocument, visit_attributes};

use super::resource::Budget;

/// Structured representation of a DOCX paragraph.
#[derive(Default, Clone)]
pub(super) struct DocxParagraph {
    pub(super) style: Option<String>,
    pub(super) runs: Vec<DocxRun>,
    pub(super) is_list_item: bool,
    pub(super) list_level: u32,
    pub(super) is_numbered: bool,
    /// Bold, italic and strikethrough that the paragraph style itself turns
    /// on. A Markdown heading does not repeat them as markers.
    pub(super) style_emphasis: Flags,
}

#[derive(Default, Clone)]
pub(super) struct DocxRun {
    pub(super) text: String,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strikethrough: bool,
    /// Resolved uppercase hex color without a leading `#`.
    pub(super) color: Option<String>,
    /// OOXML highlight color name, for example `yellow`.
    pub(super) highlight: Option<String>,
    /// Set on the run that holds a footnote or endnote reference; its text
    /// then ends with the `[^label]` marker.
    pub(super) note: Option<NoteRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoteKind {
    Footnote,
    Endnote,
}

impl NoteKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            NoteKind::Footnote => "footnote",
            NoteKind::Endnote => "endnote",
        }
    }

    /// The label of the `ordinal`-th note of this kind (1-based).
    pub(super) fn label(self, ordinal: usize) -> String {
        match self {
            NoteKind::Footnote => ordinal.to_string(),
            NoteKind::Endnote => format!("e{ordinal}"),
        }
    }
}

/// A note reference in the body. `label` is the ordinal of the note in
/// reference order (`1`, `2`, ... for footnotes, `e1`, `e2`, ... for
/// endnotes), not its `w:id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NoteRef {
    pub(super) kind: NoteKind,
    pub(super) label: String,
    /// The custom mark that follows a `w:customMarkFollows` reference.
    pub(super) mark: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FurnitureKind {
    Header,
    Footer,
}

/// A `w:headerReference` or `w:footerReference` of a section.
#[derive(Clone, Debug)]
pub(super) struct FurnitureRef {
    pub(super) kind: FurnitureKind,
    /// `default`, `first` or `even`.
    pub(super) slot: String,
    pub(super) relationship_id: String,
}

/// Note and header/footer references found while walking the body, in
/// document order. Note ids are listed in label order.
#[derive(Default)]
pub(super) struct DocxReferences {
    pub(super) footnotes: Vec<String>,
    pub(super) endnotes: Vec<String>,
    pub(super) furniture: Vec<FurnitureRef>,
    /// Custom mark per note label, from the note's first reference.
    pub(super) marks: std::collections::HashMap<String, String>,
}

#[derive(Default, Clone)]
pub(super) struct DocxTable {
    pub(super) rows: Vec<DocxRow>,
}

#[derive(Default, Clone)]
pub(super) struct DocxRow {
    pub(super) cells: Vec<DocxCell>,
}

#[derive(Default, Clone)]
pub(super) struct DocxCell {
    pub(super) paragraphs: Vec<DocxParagraph>,
}

#[derive(Clone)]
pub(super) enum DocxBlock {
    Paragraph(DocxParagraph),
    Table(DocxTable),
}

/// The text of one referenced footnote or endnote.
pub(super) struct DocxNote {
    pub(super) label: String,
    pub(super) mark: Option<String>,
    pub(super) id: String,
    pub(super) blocks: Vec<DocxBlock>,
}

/// One header or footer part, read once however many sections use it.
pub(super) struct DocxFurniture {
    pub(super) slot: String,
    pub(super) part: String,
    pub(super) blocks: Vec<DocxBlock>,
}

/// Everything outside the main story that the call extracts.
#[derive(Default)]
pub(super) struct DocxStories {
    pub(super) footnotes: Vec<DocxNote>,
    pub(super) endnotes: Vec<DocxNote>,
    pub(super) headers: Vec<DocxFurniture>,
    pub(super) footers: Vec<DocxFurniture>,
}

pub(super) struct ParsedDocxDocument {
    pub(super) blocks: Vec<DocxBlock>,
    pub(super) anchors: std::collections::HashMap<String, String>,
    pub(super) references: DocxReferences,
}

/// Walk `word/document.xml` and emit paragraphs and tables in document order,
/// with the note and header/footer references met on the way. Without
/// `mark_notes`, note references leave no marker and are not collected.
pub(super) fn parse_docx_blocks<R: BufRead>(
    xml: R,
    numbering_defs: &std::collections::HashMap<String, bool>,
    theme_colors: &std::collections::HashMap<String, String>,
    styles: &StyleSheet,
    comment_ids: Option<&std::collections::HashSet<&str>>,
    mark_notes: bool,
    budget: &mut Budget<'_>,
) -> Result<ParsedDocxDocument> {
    let mut anchors = comment_ids.map(AnchorCollector::new);
    let lookups = Lookups {
        numbering_defs,
        theme_colors,
        styles,
    };
    let mut builder = BlockBuilder::marking_notes(mark_notes);
    stories::walk(
        xml,
        "word/document.xml",
        &mut builder,
        anchors.as_mut(),
        &lookups,
        budget,
        |_, _, _, _| Ok(()),
    )?;
    let (blocks, references) = builder.finish();
    Ok(ParsedDocxDocument {
        blocks,
        anchors: anchors.map(AnchorCollector::finish).unwrap_or_default(),
        references,
    })
}
