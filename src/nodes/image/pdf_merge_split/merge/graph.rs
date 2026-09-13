use std::collections::BTreeMap;

use anyhow::{Context, Result};
use lopdf::{Document, ObjectId};

use crate::util::execution::ExecutionControl;

use super::super::page_graph::collect_page_graph;
use super::super::remap_references;

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
        Some(maximum_objects.saturating_sub(*total_objects)),
        "pdf_merge",
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
