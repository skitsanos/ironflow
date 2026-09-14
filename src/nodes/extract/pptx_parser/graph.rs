use std::collections::HashSet;

use anyhow::{Context, Result};

use super::part_paths::{relationships_part, resolve_part};
use super::presentation::slide_ids;
use super::relationships::{Kind, Relationship, Relationships, parse_pptx_rels};
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

pub(in crate::nodes::extract) struct Presentation {
    pub(super) slides: Vec<SlidePart>,
    pub(super) authors: Option<String>,
}

pub(super) struct SlidePart {
    pub(super) index: u32,
    pub(super) part: String,
    pub(super) relationships: Relationships,
    pub(super) notes: Option<String>,
    pub(super) comments: Option<String>,
}

pub(in crate::nodes::extract) fn load_presentation(
    archive: &mut Archive,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<Presentation> {
    const SOURCE: &str = "ppt/presentation.xml";
    let ids = archive.with_required_xml(SOURCE, execution, |reader| slide_ids(reader, budget))?;
    let relationships =
        archive.with_required_xml("ppt/_rels/presentation.xml.rels", execution, |reader| {
            parse_pptx_rels(reader, budget)
        })?;
    let authors = linked_part(
        archive,
        SOURCE,
        &relationships,
        Kind::Authors,
        budget,
        execution,
    )?;
    let mut slides = Vec::new();
    slides
        .try_reserve_exact(ids.len())
        .context("extract_pptx: cannot reserve slide graph")?;
    let mut seen = HashSet::new();
    for id in ids {
        budget.charge_item("PPTX presentation slides")?;
        let relationship = relationships
            .get(&id)
            .with_context(|| format!("extract_pptx: missing slide relationship '{id}'"))?;
        anyhow::ensure!(
            relationship.kind == Kind::Slide,
            "extract_pptx: relationship '{id}' is not a slide"
        );
        let part = required_target(archive, SOURCE, relationship, budget, execution)?;
        budget.charge_output(part.len() as u64, "PPTX slide part identity copy")?;
        anyhow::ensure!(
            seen.insert(part.clone()),
            "extract_pptx: duplicate presentation slide target '{part}'"
        );
        budget.charge_output(part.len() as u64 + 12, "PPTX relationship part names")?;
        let rels_name = relationships_part(&part);
        let slide_rels = archive
            .with_optional_xml(&rels_name, execution, |reader| {
                parse_pptx_rels(reader, budget)
            })?
            .unwrap_or_default();
        let notes = linked_part(archive, &part, &slide_rels, Kind::Notes, budget, execution)?;
        let comments = linked_part(
            archive,
            &part,
            &slide_rels,
            Kind::Comments,
            budget,
            execution,
        )?;
        slides.push(SlidePart {
            index: u32::try_from(slides.len() + 1).context("extract_pptx: too many slides")?,
            part,
            relationships: slide_rels,
            notes,
            comments,
        });
    }
    Ok(Presentation { slides, authors })
}

fn linked_part(
    archive: &mut Archive,
    source: &str,
    relationships: &Relationships,
    kind: Kind,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<Option<String>> {
    let mut found = None;
    for relationship in relationships.values() {
        budget.checkpoint()?;
        if relationship.kind != kind {
            continue;
        }
        anyhow::ensure!(
            found.is_none(),
            "extract_pptx: multiple {kind:?} relationships on '{source}'"
        );
        found = Some(required_target(
            archive,
            source,
            relationship,
            budget,
            execution,
        )?);
    }
    Ok(found)
}

fn required_target(
    archive: &mut Archive,
    source: &str,
    relationship: &Relationship,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<String> {
    anyhow::ensure!(
        !relationship.external,
        "extract_pptx: external {:?} relationship on '{source}' is not supported",
        relationship.kind
    );
    let part = resolve_target(source, &relationship.target, budget)?;
    archive.require_part(&part, execution)?;
    Ok(part)
}

pub(super) fn resolve_target(
    source: &str,
    target: &str,
    budget: &mut Budget<'_>,
) -> Result<String> {
    budget.admit_output(
        source.len() as u64 + target.len() as u64 + 1,
        "PPTX resolved part paths",
    )?;
    let part = resolve_part(source, target)?;
    budget.charge_output(part.len() as u64, "PPTX retained part paths")?;
    Ok(part)
}
