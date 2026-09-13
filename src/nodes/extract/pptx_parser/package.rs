use std::collections::HashMap;

use anyhow::{Context, Result};

use super::content_types::{ContentTypes, parse_content_types};
use super::graph::{Presentation, SlidePart, resolve_target};
use super::notes::parse_pptx_notes;
use super::relationships::Kind;
use super::slide::parse_pptx_slide;
use super::{PptxElement, PptxSlide};
use crate::artifacts::{ArtifactRef, LocalArtifactStore};
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

pub(in crate::nodes::extract) fn extract_pptx_slides(
    archive: &mut Archive,
    presentation: &Presentation,
    artifact_store: Option<&LocalArtifactStore>,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<Vec<PptxSlide>> {
    let content_types = if artifact_store.is_some() {
        archive
            .with_optional_xml("[Content_Types].xml", execution, |reader| {
                parse_content_types(reader, budget)
            })?
            .unwrap_or_default()
    } else {
        ContentTypes::default()
    };

    let mut slides = Vec::new();
    let mut artifacts = HashMap::new();
    let mut media = MediaState {
        artifact_store,
        content_types: &content_types,
        artifacts: &mut artifacts,
    };
    slides
        .try_reserve_exact(presentation.slides.len())
        .context("extract_pptx: cannot reserve memory for the configured number of slides")?;
    for slide in &presentation.slides {
        budget.checkpoint()?;
        budget.charge_item("PPTX slides")?;
        let (title, mut elements) =
            archive.with_required_xml(&slide.part, execution, |reader| {
                parse_pptx_slide(reader, budget)
            })?;
        resolve_images(archive, slide, &mut elements, &mut media, budget, execution)?;
        let speaker_notes = read_notes(archive, slide.notes.as_deref(), budget, execution)?;
        slides.push(PptxSlide {
            slide_index: slide.index,
            title,
            elements,
            speaker_notes,
            comments: Vec::new(),
        });
    }
    Ok(slides)
}

struct MediaState<'a> {
    artifact_store: Option<&'a LocalArtifactStore>,
    content_types: &'a ContentTypes,
    artifacts: &'a mut HashMap<String, ArtifactRef>,
}

fn resolve_images(
    archive: &mut Archive,
    slide: &SlidePart,
    elements: &mut [PptxElement],
    media: &mut MediaState<'_>,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<()> {
    for element in elements {
        budget.checkpoint()?;
        let PptxElement::Image {
            embed_id,
            embedded_path,
            artifact,
            ..
        } = element
        else {
            continue;
        };
        budget.charge_item("PPTX image elements")?;
        let Some(relationship) = embed_id
            .as_ref()
            .and_then(|embed_id| slide.relationships.get(embed_id))
            .filter(|relationship| relationship.kind == Kind::Image && !relationship.external)
        else {
            continue;
        };
        let resolved = resolve_target(&slide.part, &relationship.target, budget)?;
        budget.charge_output(resolved.len() as u64, "PPTX retained image paths")?;
        *embedded_path = Some(resolved.clone());

        if let Some(store) = media.artifact_store {
            let mime_type = media.content_types.mime_type(&resolved);
            budget.charge_output(
                descriptor_output_bytes(&mime_type),
                "PPTX artifact descriptors",
            )?;
            let descriptor = match media.artifacts.get(&resolved) {
                Some(descriptor) => descriptor.clone(),
                None => {
                    budget.charge_item("PPTX embedded media")?;
                    let descriptor = archive
                        .store_optional_part(&resolved, store, Some(mime_type), execution)?
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "extract_pptx: referenced image archive part is missing: {resolved}"
                            )
                        })?;
                    media.artifacts.insert(resolved.clone(), descriptor.clone());
                    descriptor
                }
            };
            *artifact = Some(descriptor);
        }
    }
    Ok(())
}

fn read_notes(
    archive: &mut Archive,
    notes_name: Option<&str>,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<Option<String>> {
    notes_name
        .map(|name| {
            archive.with_required_xml(name, execution, |reader| parse_pptx_notes(reader, budget))
        })
        .transpose()
        .map(|notes| notes.filter(|value| !value.trim().is_empty()))
}

fn descriptor_output_bytes(mime_type: &str) -> u64 {
    // Canonical URI prefix + two 64-byte digest spellings + JSON structure.
    18 + 64 + 64 + mime_type.len() as u64 + 64
}
