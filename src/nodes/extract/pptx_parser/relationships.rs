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

fn relationship_kind(value: &str) -> Kind {
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
