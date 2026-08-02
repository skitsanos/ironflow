use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use lopdf::{Document, Object, ObjectId};

use crate::util::execution::ExecutionControl;

use super::super::{extract_references, remap_references};

#[allow(clippy::too_many_arguments)]
pub(super) fn merge_source(
    source: Document,
    page_ids: &[ObjectId],
    merged: &mut Document,
    merged_pages_id: ObjectId,
    merged_page_ids: &mut Vec<ObjectId>,
    total_objects: &mut u64,
    maximum_objects: u64,
    execution: &ExecutionControl,
) -> Result<()> {
    let graph = collect_page_graph(
        &source,
        page_ids,
        maximum_objects.saturating_sub(*total_objects),
        execution,
    )?;
    *total_objects = total_objects
        .checked_add(graph.len() as u64)
        .ok_or_else(|| anyhow::anyhow!("pdf_merge: retained object count overflow"))?;
    if *total_objects > maximum_objects {
        anyhow::bail!(
            "pdf_merge: retained objects {} exceed IRONFLOW_MAX_PDF_MERGE_OBJECTS ({maximum_objects})",
            *total_objects
        );
    }

    let mut remap = BTreeMap::new();
    for (source_id, object) in graph {
        execution.checkpoint()?;
        remap.insert(source_id, merged.add_object(object));
    }
    for new_id in remap.values() {
        execution.checkpoint()?;
        remap_references(merged.get_object_mut(*new_id)?, &remap);
    }
    for page_id in page_ids {
        let new_page_id = remap
            .get(page_id)
            .copied()
            .with_context(|| format!("pdf_merge: page object {page_id:?} is missing"))?;
        let page = merged.get_object_mut(new_page_id)?.as_dict_mut()?;
        page.set("Parent", merged_pages_id);
        merged_page_ids.push(new_page_id);
    }
    Ok(())
}

fn collect_page_graph(
    document: &Document,
    page_ids: &[ObjectId],
    maximum: u64,
    execution: &ExecutionControl,
) -> Result<BTreeMap<ObjectId, Object>> {
    let page_set: BTreeSet<_> = page_ids.iter().copied().collect();
    let mut pending = page_ids.to_vec();
    let mut collected = BTreeMap::new();
    while let Some(id) = pending.pop() {
        execution.checkpoint()?;
        if collected.contains_key(&id) {
            continue;
        }
        if collected.len() as u64 >= maximum {
            anyhow::bail!("pdf_merge: retained objects exceed IRONFLOW_MAX_PDF_MERGE_OBJECTS");
        }
        let object = document
            .get_object(id)
            .with_context(|| format!("pdf_merge: cannot resolve object {id:?}"))?;
        pending.extend(references(object, page_set.contains(&id)));
        collected.insert(id, object.clone());
    }
    Ok(collected)
}

fn references(object: &Object, is_page: bool) -> Vec<ObjectId> {
    if is_page && let Object::Dictionary(dictionary) = object {
        let mut references = Vec::new();
        for (key, value) in dictionary.iter() {
            if key.as_slice() != b"Parent" {
                references.extend(extract_references(value));
            }
        }
        return references;
    }
    extract_references(object)
}
