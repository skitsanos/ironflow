//! PPTX view of OPC relationships: the shared parser plus PresentationML kinds.

use std::io::BufRead;

use anyhow::Result;

use crate::nodes::extract::ooxml::relationships::{self, PPTX, office_document_type};
use crate::nodes::extract::resource::Budget;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Slide,
    Notes,
    Comments,
    Authors,
    /// PowerPoint modern comments (MS-PPTX 2.1.3.13).
    ModernComments,
    /// PowerPoint modern comment authors (MS-PPTX 2.1.3.12).
    ModernAuthors,
    Image,
    Layout,
    Master,
    Other,
}

pub(super) type Relationship = relationships::Relationship<Kind>;
pub(super) type Relationships = relationships::Relationships<Kind>;

pub(super) fn parse_pptx_rels<R: BufRead>(
    xml: R,
    budget: &mut Budget<'_>,
) -> Result<Relationships> {
    relationships::parse_relationships(xml, &PPTX, relationship_kind, budget)
}

/// Relationship types PowerPoint 2019 and later define outside the
/// officeDocument namespace (MS-PPTX). Matched exactly; their targets go
/// through the same internal-only resolution as every other part.
const MODERN_COMMENTS: &str = "http://schemas.microsoft.com/office/2018/10/relationships/comments";
const MODERN_AUTHORS: &str = "http://schemas.microsoft.com/office/2018/10/relationships/authors";

fn relationship_kind(value: &str) -> Kind {
    match value {
        MODERN_COMMENTS => return Kind::ModernComments,
        MODERN_AUTHORS => return Kind::ModernAuthors,
        _ => {}
    }
    match office_document_type(value) {
        Some("slide") => Kind::Slide,
        Some("notesSlide") => Kind::Notes,
        Some("comments") => Kind::Comments,
        Some("commentAuthors") => Kind::Authors,
        Some("image") => Kind::Image,
        Some("slideLayout") => Kind::Layout,
        Some("slideMaster") => Kind::Master,
        _ => Kind::Other,
    }
}
