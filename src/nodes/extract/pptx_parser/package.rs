use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};

use super::content_types::{ContentTypes, parse_content_types};
use super::notes::parse_pptx_notes;
use super::relationships::{normalize_pptx_path, parse_pptx_rels};
use super::slide::parse_pptx_slide;
use super::{PptxElement, PptxSlide};
use crate::artifacts::{ArtifactRef, LocalArtifactStore};
use crate::nodes::extract::ooxml::Archive;
use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

pub(in crate::nodes::extract) fn extract_pptx_slides(
    archive: &mut Archive,
    artifact_store: Option<&LocalArtifactStore>,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<Vec<PptxSlide>> {
    let mut slide_parts = archive
        .entry_names("ppt/slides/slide", ".xml", execution)?
        .into_iter()
        .map(|name| slide_part(name, budget))
        .collect::<Result<Vec<_>>>()?;
    if slide_parts.is_empty() {
        anyhow::bail!("extract_pptx: presentation contains no slide parts");
    }
    slide_parts.sort_by_key(|(index, _)| *index);
    reject_duplicate_indices(&slide_parts)?;

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
        .try_reserve_exact(slide_parts.len())
        .context("extract_pptx: cannot reserve memory for the configured number of slides")?;
    for (slide_index, name) in slide_parts {
        budget.checkpoint()?;
        budget.charge_item("PPTX slides")?;
        let (title, mut elements) = archive
            .with_required_xml(&name, execution, |reader| parse_pptx_slide(reader, budget))?;
        resolve_images(
            archive,
            slide_index,
            &mut elements,
            &mut media,
            budget,
            execution,
        )?;
        let speaker_notes = read_notes(archive, slide_index, budget, execution)?;
        slides.push(PptxSlide {
            slide_index,
            title,
            elements,
            speaker_notes,
            comments: Vec::new(),
        });
    }
    Ok(slides)
}

fn slide_part(name: String, budget: &mut Budget<'_>) -> Result<(u32, String)> {
    budget.checkpoint()?;
    budget.charge_item("PPTX slide archive parts")?;
    let suffix = name
        .strip_prefix("ppt/slides/slide")
        .and_then(|value| value.strip_suffix(".xml"))
        .ok_or_else(|| anyhow::anyhow!("extract_pptx: invalid slide archive part: {name}"))?;
    let index = suffix
        .parse::<u32>()
        .with_context(|| format!("extract_pptx: invalid slide number in archive part: {name}"))?;
    if index == 0 {
        anyhow::bail!("extract_pptx: slide numbers must start at one: {name}");
    }
    Ok((index, name))
}

fn reject_duplicate_indices(parts: &[(u32, String)]) -> Result<()> {
    let mut indices = HashSet::with_capacity(parts.len());
    for (index, name) in parts {
        if !indices.insert(*index) {
            anyhow::bail!("extract_pptx: duplicate logical slide index {index}: {name}");
        }
    }
    Ok(())
}

struct MediaState<'a> {
    artifact_store: Option<&'a LocalArtifactStore>,
    content_types: &'a ContentTypes,
    artifacts: &'a mut HashMap<String, ArtifactRef>,
}

fn resolve_images(
    archive: &mut Archive,
    slide_index: u32,
    elements: &mut [PptxElement],
    media: &mut MediaState<'_>,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<()> {
    let rels_name = format!("ppt/slides/_rels/slide{slide_index}.xml.rels");
    let relationships = archive
        .with_optional_xml(&rels_name, execution, |reader| {
            parse_pptx_rels(reader, budget)
        })?
        .unwrap_or_default();

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
        let Some(target) = embed_id
            .as_ref()
            .and_then(|embed_id| relationships.get(embed_id))
        else {
            continue;
        };
        let resolved = normalize_pptx_path("ppt/slides/", target)?;
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
    slide_index: u32,
    budget: &mut Budget<'_>,
    execution: &ExecutionControl,
) -> Result<Option<String>> {
    let notes_name = format!("ppt/notesSlides/notesSlide{slide_index}.xml");
    archive
        .with_optional_xml(&notes_name, execution, |reader| {
            parse_pptx_notes(reader, budget)
        })
        .map(|notes| notes.filter(|value| !value.trim().is_empty()))
}

fn descriptor_output_bytes(mime_type: &str) -> u64 {
    // Canonical URI prefix + two 64-byte digest spellings + JSON structure.
    18 + 64 + 64 + mime_type.len() as u64 + 64
}
