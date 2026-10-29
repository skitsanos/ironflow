mod anchors;
mod blocks;
mod numbering;
mod properties;
mod theme;
mod xml;

use std::io::BufRead;

use anyhow::Result;

use anchors::AnchorCollector;
use blocks::{BlockBuilder, Lookups};
pub(super) use numbering::parse_numbering_defs;
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

pub(super) struct ParsedDocxDocument {
    pub(super) blocks: Vec<DocxBlock>,
    pub(super) anchors: std::collections::HashMap<String, String>,
}

/// Walk `word/document.xml` and emit paragraphs and tables in document order.
pub(super) fn parse_docx_blocks<R: BufRead>(
    xml: R,
    numbering_defs: &std::collections::HashMap<String, bool>,
    theme_colors: &std::collections::HashMap<String, String>,
    comment_ids: Option<&std::collections::HashSet<&str>>,
    budget: &mut Budget<'_>,
) -> Result<ParsedDocxDocument> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().check_comments = true;
    let mut buf = Vec::new();
    let mut document = XmlDocument::new("word/document.xml");
    let mut anchors = comment_ids.map(AnchorCollector::new);
    let lookups = Lookups {
        numbering_defs,
        theme_colors,
    };
    let mut builder = BlockBuilder::default();

    loop {
        budget.charge_item("DOCX XML events")?;
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|error| anyhow::anyhow!("extract_word: invalid word/document.xml: {error}"))?;
        let event = document.decode(event, budget)?;
        if let Some(anchors) = anchors.as_mut() {
            anchors.observe(&event, document.version(), budget)?;
        }
        match event {
            Event::Start(ref event) => {
                builder.open(event, false, document.version(), &lookups, budget)?;
            }
            Event::Empty(ref event) => {
                builder.open(event, true, document.version(), &lookups, budget)?;
            }
            Event::Text(ref event) => builder.text(event.as_ref()),
            Event::End(ref event) => builder.close(event.name().as_ref()),
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(ParsedDocxDocument {
        blocks: builder.finish(),
        anchors: anchors.map(AnchorCollector::finish).unwrap_or_default(),
    })
}
