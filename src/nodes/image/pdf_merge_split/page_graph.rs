use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::util::execution::ExecutionControl;

use super::extract_references;

const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

pub(super) fn collect_page_graph(
    document: &Document,
    page_ids: &[ObjectId],
    maximum: Option<u64>,
    operation: &str,
    execution: &ExecutionControl,
) -> Result<BTreeMap<ObjectId, Object>> {
    let page_set: BTreeSet<_> = page_ids.iter().copied().collect();
    let mut pending = page_set.clone();
    let mut collected = BTreeMap::new();
    while let Some(id) = pending.pop_first() {
        execution.checkpoint()?;
        if collected.contains_key(&id) {
            continue;
        }
        let source = document
            .get_object(id)
            .with_context(|| format!("{operation}: cannot resolve object {id:?}"))?;
        let is_page = page_set.contains(&id);
        // Annotation destinations and backlinks must not reintroduce the old tree.
        if !is_page && matches!(source.type_name(), Ok(b"Page" | b"Pages" | b"Catalog")) {
            continue;
        }
        if maximum.is_some_and(|maximum| collected.len() as u64 >= maximum) {
            anyhow::bail!("{operation}: retained objects exceed IRONFLOW_MAX_PDF_MERGE_OBJECTS");
        }
        let object = if is_page {
            Object::Dictionary(inherited_page(document, id, operation, execution)?)
        } else {
            source.clone()
        };
        for reference in extract_references(&object) {
            if !collected.contains_key(&reference) {
                pending.insert(reference);
            }
        }
        collected.insert(id, object);
    }
    Ok(collected)
}

fn inherited_page(
    document: &Document,
    page_id: ObjectId,
    operation: &str,
    execution: &ExecutionControl,
) -> Result<Dictionary> {
    let mut page = document.get_dictionary(page_id)?.clone();
    let mut resolved = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut current = page_id;
    loop {
        execution.checkpoint()?;
        if !visited.insert(current) {
            anyhow::bail!("{operation}: cycle in PDF page Parent chain");
        }
        let ancestor = document
            .get_dictionary(current)
            .with_context(|| format!("{operation}: cannot resolve page-tree object {current:?}"))?;
        for key in INHERITED {
            if !resolved.contains(key)
                && let Ok(value) = ancestor.get(key)
                && !matches!(document.dereference(value)?.1, Object::Null)
            {
                page.set(key, value.clone());
                resolved.insert(key);
            }
        }
        match ancestor.get(b"Parent") {
            Ok(Object::Reference(parent)) => current = *parent,
            Ok(Object::Null) | Err(lopdf::Error::DictKey(_)) => break,
            _ => anyhow::bail!("{operation}: invalid PDF page Parent reference"),
        }
    }
    for key in INHERITED {
        if !resolved.contains(key) {
            page.remove(key);
        }
    }
    page.remove(b"Parent");
    Ok(page)
}
